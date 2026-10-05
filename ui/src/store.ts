//! 微型 signal store：状态主权在 Rust，这里只承载视图层瞬时状态（ADR-0007）。

type Listener = () => void;

export function store<T>(initial: T) {
  let value = initial;
  const listeners = new Set<Listener>();
  return {
    get(): T {
      return value;
    },
    set(next: T) {
      value = next;
      listeners.forEach((fn) => fn());
    },
    update(fn: (current: T) => T) {
      this.set(fn(value));
    },
    subscribe(fn: Listener): () => void {
      listeners.add(fn);
      return () => listeners.delete(fn);
    },
  };
}
