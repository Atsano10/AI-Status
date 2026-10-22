import React from "react";
import ReactDOM from "react-dom/client";
import Appearance from "../styles/appearance";
import Bar from "./Bar";
import "./bar.css";

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <Appearance />
    <Bar />
  </React.StrictMode>,
);
