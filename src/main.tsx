import "./index.css";
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./App";
import { ErrorBoundary } from "./components/ErrorBoundary";
import { NoteWindow, noteWindowId } from "./features/notes/NoteWindow";

const root = document.getElementById("root");
if (!root) throw new Error("#root missing from main.html");

// A note opened in its own window shows only that note (owner 2026-10-25).
const noteId = noteWindowId();

createRoot(root).render(
  <StrictMode>
    <ErrorBoundary>{noteId ? <NoteWindow id={noteId} /> : <App />}</ErrorBoundary>
  </StrictMode>,
);
