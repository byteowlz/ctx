// Synthetic headless experiment only. Uses pinned Apache-2.0 FluidUse sources at build time.
@preconcurrency import CoreML
import Foundation

// Downloading is handled by the checksum-pinned shell launcher, never by inference.
public enum EvokeModelStore {
    public typealias Progress = @Sendable (String, Int64) -> Void
    public static func packageName(length: Int) -> String { "granite-embedding-30m-sparse-L\(length)-fp16" }
    public static func ensure(lengths: [Int], cacheDirectory: URL?, progress: Progress?) async throws -> URL {
        throw EvokeError.invalidAsset("Use the pinned ctx benchmark launcher to install assets")
    }
}
struct Document: Decodable {
    let id: String; let text: String; let platforms: [String]; let requires: String
}
struct Query: Decodable {
    let id: String; let text: String; let group: String; let relevant: [String]; let fixture: String; let platform: String
}
struct Fixtures: Decodable {
    let documents: [Document]; let distractors: [Document]; let queries: [Query]; let prefixes: [String]; let fixtureSha256: String
}
func now() -> UInt64 { DispatchTime.now().uptimeNanoseconds }
func ms(_ start: UInt64) -> Double { Double(now() - start) / 1e6 }
func lexical(_ text: String) -> [String] {
    text.lowercased().components(separatedBy: CharacterSet.alphanumerics.inverted).filter { !$0.isEmpty }
}
func eligible(_ doc: Document, _ query: Query) -> Bool {
    if !doc.platforms.isEmpty && !doc.platforms.contains(query.platform) { return false }
    switch doc.requires {
    case "selection": return query.fixture == "selection"
    case "audio": return query.fixture == "audio"
    case "window": return query.fixture != "unavailable"
    default: return true
    }
}
func rank(_ scores: [Int: Double]) -> [Int] {
    scores.filter { $0.value > 0 && $0.value.isFinite }.sorted {
        $0.value == $1.value ? $0.key < $1.key : $0.value > $1.value
    }.map(\.key)
}
struct BM25 {
    var postings: [String: [(Int, Double)]] = [:]
    let lengths: [Double]; let average: Double; let count: Double
    init(_ docs: [Document]) {
        let tokens = docs.map { lexical($0.text) }
        lengths = tokens.map { Double($0.count) }; count = Double(docs.count)
        average = max(1, lengths.reduce(0, +) / max(1, count))
        for (i, words) in tokens.enumerated() {
            for (term, n) in Dictionary(words.map { ($0, 1) }, uniquingKeysWith: +) {
                postings[term, default: []].append((i, Double(n)))
            }
        }
    }
    func search(_ text: String, allowed: Set<Int>) -> [Int] {
        var scores: [Int: Double] = [:]
        for term in lexical(text) {
            let entries = postings[term] ?? []
            let df = Double(entries.count), idf = log(1 + (count - df + 0.5) / (df + 0.5))
            for (id, tf) in entries where allowed.contains(id) {
                scores[id, default: 0] += idf * tf * 2.2 / (tf + 1.2 * (0.25 + 0.75 * lengths[id] / average))
            }
        }
        return rank(scores)
    }
}
struct SparseIndex {
    var postings: [Int: [(Int, Float)]] = [:]
    init(_ embeddings: [EvokeTerms]) {
        for (id, terms) in embeddings.enumerated() {
            for (term, weight) in terms { postings[term, default: []].append((id, weight)) }
        }
    }
    func search(_ terms: EvokeTerms, allowed: Set<Int>) -> [Int] {
        var scores: [Int: Double] = [:]
        // Sort terms to make fp accumulation/ties deterministic across runs.
        for term in terms.keys.sorted() {
            for (id, weight) in postings[term] ?? [] where allowed.contains(id) {
                scores[id, default: 0] += Double(terms[term]! * weight)
            }
        }
        return rank(scores)
    }
}
func hybrid(_ a: [Int], _ b: [Int]) -> [Int] {
    var scores: [Int: Double] = [:]
    for list in [a, b] { for (i, id) in list.enumerated() { scores[id, default: 0] += 1 / Double(60 + i + 1) } }
    return rank(scores)
}
func stats(_ values: [Double]) -> [String: Double] {
    let sorted = values.sorted()
    guard !sorted.isEmpty else { return [:] }
    return ["median": sorted[sorted.count / 2], "p95": sorted[min(sorted.count - 1, Int(ceil(Double(sorted.count) * 0.95)) - 1)], "min": sorted[0], "max": sorted.last!]
}
func require(_ condition: Bool, _ message: String) throws {
    if !condition { throw EvokeError.invalidAsset(message) }
}
func selfTests() throws {
    let docs = [Document(id: "a", text: "brightness monitor", platforms: [], requires: ""), Document(id: "b", text: "microphone input", platforms: [], requires: "selection")]
    let bm = BM25(docs), sparse = SparseIndex([[1: 2], [2: 4]])
    try require(bm.search("brightness", allowed: [0, 1]) == [0], "BM25 exact match")
    try require(bm.search("", allowed: [0, 1]).isEmpty, "BM25 empty")
    try require(bm.search("missing", allowed: [0, 1]).isEmpty, "BM25 missing term")
    try require(sparse.search([2: 3], allowed: [0, 1]) == [1], "Sparse dot ranking")
    try require(sparse.search([2: 3], allowed: [0]).isEmpty, "Sparse context filtering")
    try require(hybrid([0, 1], [1, 0]).count == 2, "Hybrid deduplication")
    let q = Query(id: "test", text: "x", group: "test", relevant: [], fixture: "desktop", platform: "macos")
    try require(!eligible(docs[1], q), "Selection guard")
    let transform = EvokeTransform(activeDims: 2, gamma: 1, scale: 1)
    try require(transform.terms(maxLogits: [3, -1, 5], vocabIds: [1, 2, 3]) == [1: log1p(3)], "Published pooling transform")
}
struct Metrics {
    var total = 0; var at1 = 0; var at5 = 0; var rr = 0.0; var negative = 0; var noisy = 0
    mutating func add(_ list: [Int], docs: [Document], q: Query) {
        if q.relevant.isEmpty { negative += 1; noisy += list.isEmpty ? 0 : 1; return }
        total += 1
        if let i = list.firstIndex(where: { q.relevant.contains(docs[$0].id) }) {
            at1 += i == 0 ? 1 : 0; at5 += i < 5 ? 1 : 0; rr += 1 / Double(i + 1)
        }
    }
    var json: [String: Any] { ["queries": total, "recall_at_1": Double(at1) / max(1, Double(total)), "recall_at_5": Double(at5) / max(1, Double(total)), "mrr": rr / max(1, Double(total)), "no_match_queries": negative, "no_match_nonempty_shortlists": noisy] }
}
func referenceParity(_ manager: EvokeManager, _ reference: [String: Any]) async throws -> [String: Any] {
    let atoms = reference["atoms"] as! [[String: Any]]
    var overlaps: [Double] = [], errors: [Double] = []
    for atom in atoms {
        let ids = atom["ids"] as! [Int]
        if ids.count > 64 { continue }
        let expected = Dictionary(uniqueKeysWithValues: (atom["terms"] as! [String: NSNumber]).map { (Int($0.key)!, $0.value.floatValue) })
        let result = try await manager.terms(tokenIds: ids, kind: EvokeTextKind(rawValue: atom["kind"] as! String)!).terms
        try require(result.values.allSatisfy { $0.isFinite && $0 > 0 }, "Non-finite reference embedding")
        let keys = Set(expected.keys).union(result.keys)
        let overlap = Double(Set(expected.keys).intersection(result.keys).count) / Double(max(expected.count, result.count, 1))
        let denominator = expected.values.reduce(0.0) { $0 + Double($1 * $1) }
        let error = sqrt(keys.reduce(0.0) { sum, key in let delta = Double((result[key] ?? 0) - (expected[key] ?? 0)); return sum + delta * delta } / max(denominator, 1e-12))
        overlaps.append(overlap); errors.append(error)
        try require(overlap >= 0.90 && error <= 0.05, "CoreML/ONNX sparse reference outside predeclared tolerance")
    }
    return ["cases": overlaps.count, "skipped_over64": atoms.count - overlaps.count, "min_term_overlap": overlaps.min() ?? 0, "max_relative_l2_error": errors.max() ?? 0, "tolerance_overlap": 0.90, "tolerance_relative_l2": 0.05]
}
@main struct Probe {
    static func main() async {
        do { try await run() } catch {
            let args = Array(CommandLine.arguments.dropFirst())
            if args.count == 4 {
                let failure: [String: Any] = ["status": "failed", "backend_requested": args[3], "error": error.localizedDescription]
                if let data = try? JSONSerialization.data(withJSONObject: failure, options: [.prettyPrinted, .sortedKeys]) {
                    try? data.write(to: URL(fileURLWithPath: args[2]))
                }
            }
            FileHandle.standardError.write(Data("retrieval probe failed: \(error.localizedDescription)\n".utf8))
            exit(1)
        }
    }
    static func run() async throws {
        try selfTests()
        let args = Array(CommandLine.arguments.dropFirst())
        if args == ["--self-test"] { print("retrieval primitive checks passed"); return }
        guard args.count == 4 else { throw EvokeError.invalidAsset("model-dir fixtures.json output.json ane|gpu|cpu") }
        let units: MLComputeUnits
        switch args[3] { case "ane": units = .cpuAndNeuralEngine; case "gpu": units = .cpuAndGPU; case "cpu": units = .cpuOnly; default: throw EvokeError.invalidAsset("Unknown compute units") }
        let fixtures = try JSONDecoder().decode(Fixtures.self, from: Data(contentsOf: URL(fileURLWithPath: args[1])))
        let modelDir = URL(fileURLWithPath: args[0]), loadStart = now()
        let manager = try await EvokeManager.load(from: modelDir, lengths: [64], computeUnits: units)
        let loadMs = ms(loadStart)
        // Independent HuggingFace token IDs captured upstream, not our own encoder outputs.
        let referencePath = ProcessInfo.processInfo.environment["EVOKE_TOKEN_REFERENCE"]!
        let reference = try JSONSerialization.jsonObject(with: Data(contentsOf: URL(fileURLWithPath: referencePath))) as! [String: Any]
        let tokens = reference["tokens"] as! [[String: Any]]
        for row in tokens {
            let actual = [0] + (try manager.tokenizer.encode(row["text"] as! String)) + [2]
            try require(actual == row["ids"] as! [Int], "Upstream tokenizer parity mismatch")
        }
        let firstStart = now(), first = try await manager.terms(tokenIds: manager.tokenize("light mode"), kind: .query)
        let firstMs = ms(firstStart)
        try require(first.terms.values.allSatisfy { $0.isFinite && $0 > 0 }, "Invalid first query embedding")
        var embeddings: [EvokeTerms] = [], truncated: [String] = []
        let allDocs = fixtures.documents + fixtures.distractors, indexStart = now()
        for doc in allDocs {
            let fullIds = [0] + (try manager.tokenizer.encode(doc.text)) + [2]
            if fullIds.count > 64 { truncated.append(doc.id) }
            let terms = try await manager.terms(for: doc.text, kind: .document)
            try require(terms.values.allSatisfy { $0.isFinite && $0 > 0 }, "Invalid sparse embedding for \(doc.id): \(terms.filter { !$0.value.isFinite || $0.value <= 0 })")
            embeddings.append(terms)
        }
        let documentEncodeMs = ms(indexStart)
        let parity = try await referenceParity(manager, reference)
        var corpora: [[String: Any]] = []
        for size in [28, 1000] {
            let docs = Array(allDocs.prefix(size)), constructionStart = now()
            let sparse = SparseIndex(Array(embeddings.prefix(size))), bm = BM25(docs)
            let constructionMs = ms(constructionStart)
            var metrics: [String: Metrics] = [:], rows: [[String: Any]] = []
            var timings: [String: [Double]] = [:]
            for round in 0..<3 {
                for q in fixtures.queries {
                    let fullIds = [0] + (try manager.tokenizer.encode(q.text)) + [2]
                    try require(fullIds.count <= 64, "Query truncation in frozen suite")
                    let eligibilityStart = now(), allowed = Set(docs.indices.filter { eligible(docs[$0], q) }), eligibilityMs = ms(eligibilityStart)
                    let start = now(), ids = try manager.tokenize(q.text), tokenMs = ms(start)
                    let encStart = now(), encoded = try await manager.terms(tokenIds: ids, kind: .query), encodeMs = ms(encStart)
                    try require(encoded.terms.values.allSatisfy { $0.isFinite && $0 > 0 }, "Invalid sparse query embedding for \(q.id)")
                    let sparseStart = now(), semantic = sparse.search(encoded.terms, allowed: allowed), sparseMs = ms(sparseStart)
                    let bmStart = now(), words = bm.search(q.text, allowed: allowed), bmMs = ms(bmStart)
                    let hybridStart = now(), fused = hybrid(words, semantic), hybridMs = ms(hybridStart)
                    for (name, time) in ["tokenize": tokenMs, "prediction": encoded.predictionMs, "encode_with_transform": encodeMs, "sparse_search": sparseMs, "eligibility": eligibilityMs, "bm25_end_to_end": eligibilityMs + bmMs, "sparse_end_to_end": eligibilityMs + tokenMs + encodeMs + sparseMs, "hybrid_end_to_end": eligibilityMs + tokenMs + encodeMs + sparseMs + bmMs + hybridMs] { timings[name, default: []].append(time) }
                    if round == 0 {
                        var lists: [String: Any] = [:]
                        for (name, list) in [("bm25", words), ("sparse", semantic), ("hybrid_rrf60", fused)] {
                            try require(list.allSatisfy { allowed.contains($0) }, "Context/platform scope leak")
                            metrics[name, default: Metrics()].add(list, docs: docs, q: q)
                            metrics["\(name).\(q.group)", default: Metrics()].add(list, docs: docs, q: q)
                            lists[name] = list.prefix(5).map { docs[$0].id }
                        }
                        rows.append(["id": q.id, "query": q.text, "group": q.group, "relevant": q.relevant, "results": lists])
                    }
                }
            }
            var prefixes: [[String: Any]] = []
            let allowed = Set(docs.indices.filter { eligible(docs[$0], Query(id: "prefix", text: "", group: "prefix", relevant: [], fixture: "desktop", platform: "macos")) })
            for text in fixtures.prefixes {
                let start = now(), terms = try await manager.terms(for: text, kind: .query)
                try require(terms.values.allSatisfy { $0.isFinite && $0 > 0 }, "Invalid prefix embedding")
                prefixes.append(["text": text, "top5": sparse.search(terms, allowed: allowed).prefix(5).map { docs[$0].id }, "end_to_end_ms": ms(start)])
            }
            corpora.append(["size": size, "index_construction_ms": constructionMs, "quality": metrics.mapValues(\.json), "latency_ms": timings.mapValues(stats), "queries": rows, "prefixes": prefixes])
        }
        try await Task.sleep(for: .seconds(6))
        let idleStart = now(), idle = try await manager.terms(tokenIds: manager.tokenize("screen brighter"), kind: .query)
        let idleMs = ms(idleStart)
        try require(idle.terms.values.allSatisfy { $0.isFinite && $0 > 0 }, "Invalid idle query embedding")
        let result: [String: Any] = ["fixture_sha256": fixtures.fixtureSha256, "backend_requested": args[3], "compute_units_not_placement_proof": true, "model_load_ms": loadMs, "first_encoding_ms": firstMs, "first_prediction_ms": first.predictionMs, "idle_encoding_ms": idleMs, "idle_prediction_ms": idle.predictionMs, "document_encoding_1000_ms": documentEncodeMs, "documents_truncated": truncated, "tokenizer_parity_cases": tokens.count, "onnx_reference_parity": parity, "corpora": corpora, "limits": ["Synthetic fixtures; frozen labels not real-user ground truth", "No native UI/physical keyboard/GPU-present measurement", "Compute units requested, actual operation placement unverified", "Sequential prefix queries, no async stale-query/cancellation test", "No refusals/calibration/authorization supplied by retrieval", "Index build/compile/download excluded from warm query latency"]]
        let data = try JSONSerialization.data(withJSONObject: result, options: [.prettyPrinted, .sortedKeys])
        try data.write(to: URL(fileURLWithPath: args[2]))
        print("retrieval benchmark \(args[3]) finished; output \(args[2])")
    }
}
