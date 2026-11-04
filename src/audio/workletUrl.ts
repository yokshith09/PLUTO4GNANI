// The bundled capture worklet's URL. In its own module so tests can replace it
// (`vi.mock("./workletUrl")`) without asking Vitest to build a worker. Owner: D1 (T-012).
export { default } from "./capture.worklet.ts?worker&url";
