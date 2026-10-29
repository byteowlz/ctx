"""Synthetic labelled utterances for the command-vs-dictation gate (no private data).

Groups:
  short  - 1-4 words
  long   - full sentences
  hard   - dictated text that reads like an instruction, or a command phrased like prose
  quoted - commands embedded in reported speech (dictation)
"""
C, D = "command", "dictation"

CASES = [
    # --- English, short commands
    ("lock the screen", C, "en", "short"),
    ("mute", C, "en", "short"),
    ("dark mode", C, "en", "short"),
    ("volume up", C, "en", "short"),
    ("open display settings", C, "en", "short"),
    ("switch to agent three", C, "en", "short"),
    ("take a screenshot", C, "en", "short"),
    ("next branch", C, "en", "short"),
    ("go back", C, "en", "short"),
    ("show the tree", C, "en", "short"),
    ("copper whale", C, "en", "short"),  # spoken disambiguation code
    ("cancel that", C, "en", "short"),
    ("turn on do not disturb", C, "en", "short"),
    ("search issues", C, "en", "short"),
    # --- English, short dictation
    ("thanks", D, "en", "short"),
    ("sounds good", D, "en", "short"),
    ("see you tomorrow", D, "en", "short"),
    ("okay", D, "en", "short"),
    ("best regards", D, "en", "short"),
    ("happy birthday", D, "en", "short"),
    ("no worries at all", D, "en", "short"),
    ("I agree", D, "en", "short"),
    ("love it", D, "en", "short"),
    ("running late", D, "en", "short"),
    ("great job everyone", D, "en", "short"),
    ("what time works for you", D, "en", "short"),
    # --- English, long commands
    ("can you please switch the display to dark mode and dim it a bit", C, "en", "long"),
    ("I want to change my keyboard layout to German", C, "en", "long"),
    ("could you open the privacy settings so I can check microphone access", C, "en", "long"),
    ("switch over to the agent that is working on the release pipeline", C, "en", "long"),
    ("take a screenshot of the current window and save it to the capture folder", C, "en", "long"),
    ("please turn the volume down, it is way too loud right now", C, "en", "long"),
    ("show me the issues that are still open for the ctx bar", C, "en", "long"),
    ("go back one level and pick the second option instead", C, "en", "long"),
    # --- English, long dictation
    ("The meeting went well and we agreed to ship the first version next week.", D, "en", "long"),
    ("I think the new layout looks much cleaner than the previous one, nice work.", D, "en", "long"),
    ("We still need to decide who owns the speaker verification before we start.", D, "en", "long"),
    ("My train was delayed by forty minutes, so I will join the call a bit later.", D, "en", "long"),
    ("The results from yesterday look promising, but the sample size is still small.", D, "en", "long"),
    ("Could we move the review to Thursday afternoon instead of Wednesday?", D, "en", "long"),
    ("Honestly I did not expect the latency to be this low on the small model.", D, "en", "long"),
    ("Let me know if you have any questions about the proposal.", D, "en", "long"),
    # --- English, hard: dictated text that sounds like an instruction
    ("Please send me the report by Friday.", D, "en", "hard"),
    ("Turn left at the second traffic light and park behind the building.", D, "en", "hard"),
    ("Open the window if it gets too warm in the lab.", D, "en", "hard"),
    ("Remember to lock the door when you leave.", D, "en", "hard"),
    ("Restart the server before the demo, otherwise it will be slow.", D, "en", "hard"),
    ("Switch off the lights in the meeting room after you are done.", D, "en", "hard"),
    ("Delete the old drafts, we do not need them anymore.", D, "en", "hard"),
    ("Mute your microphone when you are not speaking.", D, "en", "hard"),
    # --- English, hard: commands phrased like prose
    ("it is too bright in here", C, "en", "hard"),
    ("I can barely hear anything", C, "en", "hard"),
    ("my screen is way too dark", C, "en", "hard"),
    ("I need some quiet for the next hour", C, "en", "hard"),
    # --- English, quoted commands inside dictation
    ("He told me to lock the screen whenever I leave my desk.", D, "en", "quoted"),
    ("She said turn on dark mode, it is easier on the eyes.", D, "en", "quoted"),
    ("The manual says to restart the computer after the update.", D, "en", "quoted"),
    ("Tell Paul that the volume button is broken again.", D, "en", "quoted"),
    # --- German, short commands
    ("Bildschirm sperren", C, "de", "short"),
    ("stumm schalten", C, "de", "short"),
    ("Dunkelmodus", C, "de", "short"),
    ("lauter", C, "de", "short"),
    ("Bildschirmfoto machen", C, "de", "short"),
    ("zu Agent drei wechseln", C, "de", "short"),
    ("zurück", C, "de", "short"),
    ("Einstellungen öffnen", C, "de", "short"),
    ("abbrechen", C, "de", "short"),
    ("Baum anzeigen", C, "de", "short"),
    # --- German, short dictation
    ("danke", D, "de", "short"),
    ("klingt gut", D, "de", "short"),
    ("bis morgen", D, "de", "short"),
    ("alles klar", D, "de", "short"),
    ("viele Grüße", D, "de", "short"),
    ("gern geschehen", D, "de", "short"),
    ("bin gleich da", D, "de", "short"),
    ("das passt mir gut", D, "de", "short"),
    ("herzlichen Glückwunsch", D, "de", "short"),
    ("wann hast du Zeit", D, "de", "short"),
    # --- German, long commands
    ("kannst du bitte den Bildschirm auf Dunkelmodus umstellen und etwas dunkler machen", C, "de", "long"),
    ("ich möchte das Tastaturlayout auf Englisch ändern", C, "de", "long"),
    ("öffne bitte die Datenschutzeinstellungen, ich will den Mikrofonzugriff prüfen", C, "de", "long"),
    ("wechsel zu dem Agenten, der an der Release-Pipeline arbeitet", C, "de", "long"),
    ("mach bitte die Lautstärke leiser, das ist gerade viel zu laut", C, "de", "long"),
    ("zeig mir die offenen Issues für die ctx bar", C, "de", "long"),
    # --- German, long dictation
    ("Das Meeting lief gut und wir haben beschlossen, die erste Version nächste Woche auszuliefern.", D, "de", "long"),
    ("Ich finde das neue Layout viel aufgeräumter als das alte, gute Arbeit.", D, "de", "long"),
    ("Mein Zug hatte vierzig Minuten Verspätung, ich komme etwas später in den Call.", D, "de", "long"),
    ("Die Ergebnisse von gestern sehen vielversprechend aus, aber die Stichprobe ist noch klein.", D, "de", "long"),
    ("Können wir das Review auf Donnerstagnachmittag verschieben?", D, "de", "long"),
    ("Sag mir Bescheid, wenn du Fragen zum Vorschlag hast.", D, "de", "long"),
    # --- German, hard: dictated imperatives
    ("Schick mir bitte den Bericht bis Freitag.", D, "de", "hard"),
    ("Mach das Fenster auf, wenn es im Labor zu warm wird.", D, "de", "hard"),
    ("Denk daran, die Tür abzuschließen, wenn du gehst.", D, "de", "hard"),
    ("Starte den Server vor der Demo neu, sonst ist er langsam.", D, "de", "hard"),
    ("Lösch die alten Entwürfe, die brauchen wir nicht mehr.", D, "de", "hard"),
    ("Schalte dein Mikrofon stumm, wenn du nicht sprichst.", D, "de", "hard"),
    # --- German, hard: prose commands
    ("es ist viel zu hell hier", C, "de", "hard"),
    ("ich höre kaum etwas", C, "de", "hard"),
    ("mein Bildschirm ist viel zu dunkel", C, "de", "hard"),
    # --- German, quoted
    ("Er hat gesagt, ich soll den Bildschirm sperren, wenn ich weggehe.", D, "de", "quoted"),
    ("Im Handbuch steht, man soll den Rechner nach dem Update neu starten.", D, "de", "quoted"),
    ("Sag Paul, dass die Lautstärketaste wieder kaputt ist.", D, "de", "quoted"),
]

# Imperative/command stems for the lexical baseline (EN + DE).
VERBS = {
    "lock", "mute", "unmute", "open", "close", "switch", "take", "turn", "show", "go", "cancel",
    "search", "volume", "dark", "light", "next", "back", "change", "dim", "restart", "start", "stop",
    "pick", "select", "launch", "set", "increase", "decrease", "play", "pause", "copper",
    "bildschirm", "stumm", "dunkelmodus", "lauter", "leiser", "bildschirmfoto", "zu", "zurück",
    "einstellungen", "abbrechen", "baum", "öffne", "mach", "zeig", "wechsel", "wechsle", "schalte",
    "starte", "sperren",
}
