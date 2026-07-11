import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { I18nProvider } from "./i18n";
import "./styles.css";

ReactDOM.createRoot(document.getElementById("app") as HTMLElement).render(
  <I18nProvider>
    <App />
  </I18nProvider>
);
