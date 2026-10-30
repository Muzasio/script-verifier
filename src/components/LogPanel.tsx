import { useEffect, useRef } from "react";

export default function LogPanel({ lines }: { lines: string[] }) {
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const el = ref.current;
    if (el) el.scrollTop = el.scrollHeight;
  }, [lines.length]);

  return (
    <div className="log" ref={ref}>
      {lines.length === 0 ? (
        <div className="empty small">no output yet</div>
      ) : (
        lines.map((line, i) => (
          <div className="log__line" key={i}>
            {line}
          </div>
        ))
      )}
    </div>
  );
}
