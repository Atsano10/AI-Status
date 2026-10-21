// Dev-only page that draws every bar state with sample data, so the design can be checked in a browser
// without running Tauri. Open http://localhost:1420/gallery.html while `npm run dev` is running.
import React from "react";
import ReactDOM from "react-dom/client";
import GalleryPage from "./GalleryPage";
// Font candidates for the "Fonts" section. Only the one we pick moves into the real bar.
import "@fontsource/geist-sans/400.css";
import "@fontsource/geist-sans/500.css";
import "@fontsource/geist-sans/700.css";
import "@fontsource/jetbrains-mono/400.css";
import "@fontsource/jetbrains-mono/500.css";
import "@fontsource/jetbrains-mono/700.css";
import "@fontsource/nunito/500.css";
import "@fontsource/nunito/600.css";
import "@fontsource/nunito/800.css";
import "@fontsource/pixelify-sans/400.css";
import "@fontsource/pixelify-sans/500.css";
import "@fontsource/pixelify-sans/700.css";
import "./bar.css";
import "./gallery.css";

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <GalleryPage />
  </React.StrictMode>,
);
