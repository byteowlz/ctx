import json, statistics, sys
rows = [json.loads(l) for l in open(sys.argv[1])]
errs = sum(1 for r in rows for k in r if k.endswith(":err"))
print(f"rows={len(rows)} errors={errs}  commands={sum(r['label']=='command' for r in rows)} dictation={sum(r['label']=='dictation' for r in rows)}")

def acc(pred_fn, subset):
    ok = [pred_fn(r) == r["label"] for r in subset]
    return f"{100*sum(ok)/len(ok):5.1f}% ({sum(ok)}/{len(ok)})"

groups = ["short", "long", "hard", "quoted"]
methods = [("length<=4", lambda r: r["base:length<=4"]), ("lexical", lambda r: r["base:lexical"])]
for route in ["jev", "d3-lite"]:
    for ctx in ["none", "editor"]:
        k = f"{route}:{ctx}:p"
        methods.append((f"{route}/{ctx}", lambda r, k=k: "command" if r.get(k, 0) >= 0.5 else "dictation"))
print(f"\n{'method':16} {'all':16}" + "".join(f"{g:16}" for g in groups) + f"{'en':16}{'de':16}")
for name, fn in methods:
    line = f"{name:16} {acc(fn, rows):16}"
    for g in groups:
        line += f"{acc(fn, [r for r in rows if r['group']==g]):16}"
    for lang in ["en", "de"]:
        line += f"{acc(fn, [r for r in rows if r['lang']==lang]):16}"
    print(line)

print("\nlatency ms (sequential, HTTPS via gateway):")
for route in ["jev", "d3-lite"]:
    ms = sorted(r[f"{route}:none:ms"] for r in rows if f"{route}:none:ms" in r)
    print(f"  {route:8} p50={statistics.median(ms):6.0f}  p90={ms[int(.9*len(ms))]:6.0f}  p99={ms[min(len(ms)-1,int(.99*len(ms)))]:6.0f}  min={ms[0]:5.0f}")

print("\nselective (abstain band: decide only if p>=hi or p<=1-hi; abstain -> dictation):")
for route in ["jev", "d3-lite"]:
    k = f"{route}:none:p"
    for hi in [0.8, 0.9, 0.97]:
        decided = [r for r in rows if r[k] >= hi or r[k] <= 1 - hi]
        cov = len(decided) / len(rows)
        wrong = sum(("command" if r[k] >= hi else "dictation") != r["label"] for r in decided)
        false_cmd = sum(r[k] >= hi and r["label"] == "dictation" for r in rows)
        print(f"  {route:8} hi={hi:.2f} coverage={100*cov:5.1f}%  errors_when_decided={wrong}  dictation_taken_as_command={false_cmd}")

print("\nmisclassified by jev/none and d3-lite/none:")
for r in rows:
    j = "command" if r["jev:none:p"] >= .5 else "dictation"
    d = "command" if r["d3-lite:none:p"] >= .5 else "dictation"
    if j != r["label"] or d != r["label"]:
        print(f"  [{r['label'][:3]} {r['group']:6} {r['lang']}] jev={r['jev:none:p']:.2f} d3={r['d3-lite:none:p']:.2f} jev+ed={r['jev:editor:p']:.2f}  {r['text'][:70]}")
