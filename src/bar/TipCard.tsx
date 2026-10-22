import { useEffect, useState } from "react";
import type { Tip } from "../bindings/Tip";

interface TipCardProps {
  tip: Tip;
  /** Copy the tip's command. Left out in the dev gallery. */
  onCopy?: (text: string) => void;
  /** Hide this kind of tip for an hour, or for good. */
  onHide?: (forever: boolean) => void;
}

/** One tip from crates/core/src/tips.rs, in the panel: what to do, why, and a button for it. */
export default function TipCard({ tip, onCopy, onHide }: TipCardProps) {
  const [copied, setCopied] = useState(false);
  useEffect(() => {
    if (!copied) return;
    const timer = setTimeout(() => setCopied(false), 1500);
    return () => clearTimeout(timer);
  }, [copied]);

  return (
    <div className={`tip tip--${tip.severity}`}>
      <TipIcon severity={tip.severity} />
      <div className="tip-body">
        <p className="tip-text">{tip.text}</p>
        <span className="tip-why">{tip.why}</span>
        <div className="tip-actions">
          {tip.action?.kind === "copy" && (
            <button
              className="btn"
              onClick={() => {
                onCopy?.(tip.action!.text);
                setCopied(true);
              }}
            >
              {copied ? "Copied" : `Copy ${tip.action.text}`}
            </button>
          )}
          <span className="tip-dismiss">
            <button onClick={() => onHide?.(false)}>Hide</button>
            <button onClick={() => onHide?.(true)}>Never</button>
          </span>
        </div>
      </div>
    </div>
  );
}

/** A pixel "!" for warnings, a lightbulb for nudges, an "i" for the rest. */
function TipIcon({ severity }: { severity: Tip["severity"] }) {
  const pixels: Record<Tip["severity"], [number, number, number, number][]> = {
    warning: [
      [3, 0, 2, 6],
      [3, 7, 2, 2],
    ],
    nudge: [
      [2, 0, 4, 1],
      [1, 1, 6, 4],
      [2, 5, 4, 1],
      [3, 6, 2, 1],
      [2, 7, 4, 2],
    ],
    info: [
      [3, 0, 2, 2],
      [3, 3, 2, 6],
    ],
  };
  return (
    <svg
      className="tip-icon"
      width="16"
      height="18"
      viewBox="0 0 8 9"
      shapeRendering="crispEdges"
      aria-hidden
    >
      {pixels[severity].map(([x, y, w, h]) => (
        <rect key={`${x},${y}`} x={x} y={y} width={w} height={h} />
      ))}
    </svg>
  );
}
