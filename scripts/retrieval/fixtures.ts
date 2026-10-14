// Frozen before model scoring. Synthetic candidates only; no inferred labels.
import { createHash } from "node:crypto";
export function fixtures(catalog: { items: any[] }) {
  const paraphrases: Record<string, string[]> = {
    "appearance.light": ["make the operating system use a pale theme", "use a bright desktop appearance"],
    "appearance.dark": ["make the operating system use a dark theme", "give the desktop a black appearance"],
    "appearance.open": ["where can I change the desktop wallpaper", "show the controls for desktop colors"],
    "theme.pick": ["browse the looks installed in Omarchy", "pick another Omarchy color scheme"],
    "sound.volume": ["my computer is too loud", "silence the speakers"],
    "sound.output": ["send computer sound to my headset instead", "change which speaker plays computer audio"],
    "sound.input": ["my recording uses the wrong microphone", "change which device captures my voice"],
    "display.brightness": ["the monitor is too dim", "make the screen brighter"],
    "display.open": ["the letters on my monitor are too small", "arrange my two monitors side by side"],
    "display.night": ["reduce the blue light from my screen tonight", "give the screen a warmer tint"],
    "notifications.focus": ["stop notifications while I concentrate", "I do not want interruptions"],
    "network.wifi": ["join another wireless connection", "choose the router for this computer"],
    "network.bluetooth": ["pair a wireless mouse", "connect a new Bluetooth headset"],
    "network.vpn": ["show my configured encrypted network tunnel", "inspect my virtual private network connection"],
    "system.settings": ["show the computer preferences", "where are the operating system configuration controls"],
    "system.lock": ["secure my desktop while I step away", "require a password before anyone can use this session"],
    "system.power": ["keep the computer awake longer", "change the computer energy saving options"],
    "files.open": ["browse the folders on my computer", "show me Finder"],
    "clipboard.open": ["see what I copied earlier", "show my paste history"],
    "screenshot.capture": ["save a picture of this window", "grab an image of the visible desktop"],
    "selection.speak": ["say the highlighted passage out loud", "listen to the text I selected"],
    "audio.transcribe": ["write down the words in this available recording", "convert this audio file into a transcript"],
    "dictation.start": ["let me speak instead of typing", "turn my spoken words into typed text"],
    "oqto.open": ["take me to my agent workspace", "show the Oqto interface"],
    "tasks.add": ["remind me to buy groceries", "put review budget on my todo list"],
    "history.search": ["find the conversation where we discussed Rust", "look up a previous agent session"],
    "memory.search": ["what preference did I save about fonts", "find a remembered operational fact"],
    "issues.search": ["look up the tracked defect for startup", "find an existing feature ticket"],
  };
  const documents = catalog.items.map(item => ({ id: item.id, text: `${item.label}. ${item.description} Tool: ${item.tool}. Interface: ${item.interface}.`, platforms: item.platforms ?? [], requires: item.requires ?? "" }));
  const queries = catalog.items.flatMap(item => {
    const fixture = item.requires === "selection" ? "selection" : item.requires === "audio" ? "audio" : "desktop";
    const platform = item.platforms?.[0] ?? "macos";
    return [item.label, ...paraphrases[item.id]].map((text, index) => ({ id: `${item.id}.${index}`, text, group: index === 0 ? "literal" : "paraphrase", relevant: [item.id], fixture, platform }));
  });
  for (const text of ["order a pizza", "book a flight to Tokyo", "diagnose a broken ankle", "change my bank account password", "solve this integral", "translate a novel into Japanese", "buy a concert ticket", "forecast tomorrow's weather"]) {
    queries.push({ id: `none.${queries.length}`, text, group: "no-match", relevant: [], fixture: "desktop", platform: "macos" });
  }
  const workspaces = ["Harbor", "Quartz", "Cedar", "Maple", "Orchid", "Slate", "Amber", "Birch", "Coral", "Dune", "Elm", "Flint", "Grove", "Hazel", "Iris", "Juniper", "Kelp", "Larch"];
  const actions = [
    "Export invoice records as CSV", "Generate a quarterly revenue report", "Archive completed customer orders", "Create a purchase order", "Inspect unpaid invoices", "Change the invoice currency",
    "Schedule a team meeting", "Cancel a calendar appointment", "Reschedule an appointment", "Find an available meeting room", "Export calendar events", "Set a recurring event",
    "Invite a project collaborator", "Remove a project collaborator", "Change document sharing permissions", "Copy a document sharing link", "View document edit history", "Restore a document revision",
    "Run the project test suite", "Build a software release", "Inspect failed build logs", "Restart a development server", "Format source files", "Find unused project dependencies",
    "Create a database backup", "Restore a database backup", "Inspect database query latency", "Export a database table", "View database connection settings", "Rotate a database client credential",
    "Create a support ticket", "Assign a support ticket", "Escalate a support ticket", "Close a support ticket", "Export support tickets", "Find an archived support ticket",
    "Resize a product photograph", "Convert an image to PNG", "Compress a product image", "Crop a product photograph", "Export a vector graphic", "Remove image metadata",
    "Change the editor color theme", "Mute project discussion notifications", "Set a dashboard refresh interval", "Configure a project webhook", "Inspect a project audit log", "Export project settings",
    "Sort spreadsheet rows", "Calculate a spreadsheet subtotal", "Filter spreadsheet records", "Create a spreadsheet chart", "Import spreadsheet data", "Freeze spreadsheet header rows",
  ];
  const distractors = workspaces.flatMap(workspace => actions.map((action, index) => ({ id: `fixture.${workspace.toLowerCase()}.${index}`, text: `${action} in ${workspace}. Operates on the synthetic ${workspace} workspace.`, platforms: [], requires: "" })));
  if (distractors.length !== 972 || documents.length !== 28) throw new Error("Frozen corpus size drift");
  for (const query of queries) for (const id of query.relevant) if (!documents.some(doc => doc.id === id)) throw new Error("Missing relevance target");
  const data = { version: 1, status: "synthetic-retrieval-only-no-execution", documents, distractors, queries, prefixes: ["l", "li", "lig", "light", "light m", "light mode", "m", "mi", "mic", "microphone", "s", "sc", "screen", "screen brighter"], tokenizerCases: ["", "light mode", "Wi-Fi", "C++ & Rust", "café", "cafe\u0301", "日本語", "123.45", "say <mask> now", "can't", " foo\nbar"] };
  return { ...data, fixtureSha256: createHash("sha256").update(JSON.stringify(data)).digest("hex") };
}
if (import.meta.main) {
  const path = Bun.argv[2];
  if (!path) throw new Error("Output path required");
  const catalog = await Bun.file(new URL("./benchmark-catalog.json", import.meta.url)).json();
  await Bun.write(path, JSON.stringify(fixtures(catalog), null, 2) + "\n");
}
