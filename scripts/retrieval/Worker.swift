// Runtime adapter over pinned Apache-2.0 FluidUse. No download, execution or native UI.
@preconcurrency import CoreML
import CryptoKit
import Foundation

public enum EvokeModelStore {
    public typealias Progress = @Sendable (String, Int64) -> Void
    public static func packageName(length: Int) -> String { "granite-embedding-30m-sparse-L\(length)-fp16" }
    public static func ensure(lengths: [Int], cacheDirectory: URL?, progress: Progress?) async throws -> URL {
        throw EvokeError.invalidAsset("Install the pinned ctx retrieval assets explicitly")
    }
}
struct RuntimeDocument: Codable, Equatable { let id: String; let text: String }
struct RuntimeInput: Decodable { let id: Int; let query: String; let documents: [RuntimeDocument] }
struct RuntimeOutput: Encodable {
    let id: Int; let ids: [String]; let truncated: Bool; let latencyMs: Double; let error: String?
}
struct Assets: Decodable { let files: [String: String] }
func emit<T: Encodable>(_ output: T) throws {
    let data = try JSONEncoder().encode(output)
    FileHandle.standardOutput.write(data + Data([10]))
}
func requireFinite(_ terms: EvokeTerms) throws {
    guard terms.values.allSatisfy({ $0.isFinite && $0 > 0 }) else {
        throw EvokeError.predictionFailed("Non-finite sparse weights")
    }
}
func fullTokens(_ text: String, _ manager: EvokeManager) throws -> [Int] {
    [0] + (try manager.tokenizer.encode(text)) + [2]
}
func verifyAssets(_ directory: URL, manifest: URL) throws {
    let assets = try JSONDecoder().decode(Assets.self, from: Data(contentsOf: manifest))
    // EvokeManager prefers .mlmodelc: never load an unverified precompiled override.
    let compiled = directory.appendingPathComponent(EvokeModelStore.packageName(length: 64) + ".mlmodelc")
    guard !FileManager.default.fileExists(atPath: compiled.path) else {
        throw EvokeError.invalidAsset("Unverified compiled model override")
    }
    for (path, expected) in assets.files {
        let data = try Data(contentsOf: directory.appendingPathComponent(path), options: .mappedIfSafe)
        let actual = SHA256.hash(data: data).map { String(format: "%02x", $0) }.joined()
        guard actual == expected else { throw EvokeError.invalidAsset("Pinned asset checksum mismatch") }
    }
}
struct RuntimeIndex {
    let documents: [RuntimeDocument]
    var postings: [Int: [(Int, Float)]] = [:]
    init(_ documents: [RuntimeDocument], manager: EvokeManager) async throws {
        self.documents = documents
        for (index, document) in documents.enumerated() {
            // Do not silently index a truncated capability declaration.
            guard try fullTokens(document.text, manager).count <= 64 else {
                throw EvokeError.invalidAsset("Capability exceeds the installed L64 token budget")
            }
            let terms = try await manager.terms(for: document.text, kind: .document)
            try requireFinite(terms)
            for (term, weight) in terms { postings[term, default: []].append((index, weight)) }
        }
    }
    func search(_ terms: EvokeTerms) -> [String] {
        var scores: [Int: Double] = [:]
        for term in terms.keys.sorted() {
            for (index, weight) in postings[term] ?? [] {
                scores[index, default: 0] += Double(terms[term]! * weight)
            }
        }
        return scores.filter { $0.value.isFinite && $0.value > 0 }.sorted {
            $0.value == $1.value ? documents[$0.key].id < documents[$1.key].id : $0.value > $1.value
        }.prefix(5).map { documents[$0.key].id }
    }
}
func validInput(_ input: RuntimeInput) -> Bool {
    input.id >= 0 && !input.query.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
        && input.query.unicodeScalars.count <= 512 && input.documents.count <= 128
        && Set(input.documents.map(\.id)).count == input.documents.count
        && input.documents.allSatisfy { !$0.id.isEmpty && $0.id.utf8.count <= 256 && $0.text.utf8.count <= 4096 }
}
@main struct Worker {
    static func main() async {
        do { try await run() } catch {
            // Never echo request text or private asset paths into service logs.
            FileHandle.standardError.write(Data("ctx retrieval worker unavailable\n".utf8)); exit(1)
        }
    }
    static func run() async throws {
        let args = Array(CommandLine.arguments.dropFirst())
        guard args.count == 3, ["cpu", "ane"].contains(args[2]) else {
            throw EvokeError.invalidAsset("model-dir assets.json cpu|ane required")
        }
        let directory = URL(fileURLWithPath: args[0])
        try verifyAssets(directory, manifest: URL(fileURLWithPath: args[1]))
        let manager = try await EvokeManager.load(from: directory, lengths: [64], computeUnits: args[2] == "cpu" ? .cpuOnly : .cpuAndNeuralEngine)
        try emit(["ready": true])
        var index: RuntimeIndex?
        while let line = readLine() {
            guard line.utf8.count <= 512 * 1024 else { throw EvokeError.invalidAsset("Input too large") }
            let input = try JSONDecoder().decode(RuntimeInput.self, from: Data(line.utf8))
            guard validInput(input) else { throw EvokeError.invalidAsset("Invalid input") }
            let start = DispatchTime.now().uptimeNanoseconds
            do {
                let ids = try fullTokens(input.query, manager)
                // Query truncation does not pretend to be a successful retrieval.
                guard ids.count <= 64 else {
                    try emit(RuntimeOutput(id: input.id, ids: [], truncated: true, latencyMs: 0, error: "Query exceeds L64 token budget")); continue
                }
                if index?.documents != input.documents { index = try await RuntimeIndex(input.documents, manager: manager) }
                let terms = try await manager.terms(tokenIds: ids, kind: .query).terms
                try requireFinite(terms)
                let latency = Double(DispatchTime.now().uptimeNanoseconds - start) / 1e6
                try emit(RuntimeOutput(id: input.id, ids: index?.search(terms) ?? [], truncated: false, latencyMs: latency, error: nil))
            } catch {
                index = nil
                try emit(RuntimeOutput(id: input.id, ids: [], truncated: false, latencyMs: 0, error: "Local retrieval failed; Jev remains available"))
            }
        }
    }
}
