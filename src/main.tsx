import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import "./styles.css";
import "./i18n";
import { App } from "./app/App";

const root = document.getElementById("root");

if (!root) {
  throw new Error("Root element #root was not found.");
}

createRoot(root).render(
  <StrictMode>
    <App />
  </StrictMode>,
);

