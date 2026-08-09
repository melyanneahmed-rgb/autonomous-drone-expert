import { StrictMode } from "react";
import { createRoot } from "react-dom/client";

import App from "./App";
import { registerPwa } from "./pwa-register";
import "./styles.css";

const root = document.getElementById("root");

if (!root) {
  throw new Error("Smart Configurator root element is missing");
}

createRoot(root).render(
  <StrictMode>
    <App />
  </StrictMode>,
);

registerPwa();
