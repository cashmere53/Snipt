import { getCurrentWindow } from "@tauri-apps/api/window";
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { WindowRouter } from "@/app/WindowRouter";

const root = document.getElementById("root");
if (!root) throw new Error("#root element is missing");

createRoot(root).render(
  <StrictMode>
    <WindowRouter label={getCurrentWindow().label} />
  </StrictMode>,
);
