// Vitest setup for every frontend test file.
import { configure } from "@testing-library/react";

// `findBy…` and `waitFor` give up after 1 s by default. Screens that need two or three queries
// to arrive (e.g. the "Yesterday" line on Today) can take longer than that on a slow CI runner:
// on 2026-10-13 one did on macOS (1.3 s) while passing locally and on Windows. 5 s still fails
// fast on a real bug; the per-test limit (vite.config.ts) is 15 s.
configure({ asyncUtilTimeout: 5_000 });
