// Test helpers for src/audio (the lint forbids `!` non-null assertions). Owner: D1 (T-012).

/** Element `i`, or a test failure naming it. */
export function nth<T>(items: ArrayLike<T>, i: number): T {
  const v = items[i];
  if (v === undefined) throw new Error(`expected an element at index ${i}, found ${items.length} elements`);
  return v;
}

/** The value, or a test failure if it is null or undefined. */
export function must<T>(value: T | null | undefined): T {
  if (value === null || value === undefined) throw new Error("expected a value, got nothing");
  return value;
}
