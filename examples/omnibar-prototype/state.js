export class SelectionSession {
  constructor() { this.generation = 0; this.items = []; this.active = 0; this.deadline = null; this.receipt = null; }
  invalidate() {
    this.generation++; this.items = []; this.active = 0; this.deadline = null; this.receipt = null;
    return this.generation;
  }
  accept(generation, items, timeoutMs, now) {
    if (generation !== this.generation) return false;
    this.items = items; this.active = 0; this.receipt = null;
    this.deadline = items.length && timeoutMs ? now + timeoutMs : null;
    return true;
  }
  cancelTimeout() { this.deadline = null; }
  move(delta) {
    this.cancelTimeout();
    this.active = Math.max(0, Math.min(this.items.length - 1, this.active + delta));
  }
  choose(index = this.active, reason = "manual") {
    if (this.receipt || !this.items[index]) return null;
    this.cancelTimeout(); this.active = index;
    this.receipt = { item: this.items[index], reason };
    return this.receipt;
  }
  expire(now) { return this.deadline !== null && now >= this.deadline ? this.choose(0, "timeout") : null; }
}
