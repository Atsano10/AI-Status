// Dev-only page that draws every bar state with sample data, so the design can be checked in a browser
// without running Tauri. Open http://localhost:1420/gallery.html while `npm run dev` is running.
import React from "react";
import ReactDOM from "react-dom/client";
import GalleryPage from "./GalleryPage";
import "./bar.css";
import "./gallery.css";

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <GalleryPage />
  </React.StrictMode>,
);
