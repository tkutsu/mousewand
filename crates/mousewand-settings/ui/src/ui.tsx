// The building blocks every page shares, after macOS System Settings.

import type { ReactNode } from "react";

/** A titled group of rows, with an optional note under it. */
export function Section({
  title,
  note,
  action,
  children,
}: {
  title?: string;
  note?: ReactNode;
  action?: ReactNode;
  children: ReactNode;
}) {
  return (
    <section className="flex flex-col gap-1.5">
      {(title || action) && (
        <div className="flex min-h-6 items-center justify-between px-1">
          <h2 className="text-[13px] font-semibold">{title}</h2>
          {action}
        </div>
      )}
      <div className="divide-y divide-line overflow-hidden rounded-[10px] border border-line bg-group">{children}</div>
      {note && <p className="px-1 text-[11px] leading-relaxed text-ink-2">{note}</p>}
    </section>
  );
}

/** A row in a group: what it is on the left, the control on the right. */
export function Row({ label, hint, children }: { label: ReactNode; hint?: ReactNode; children: ReactNode }) {
  return (
    <div className="flex min-h-11 items-center gap-4 px-3 py-2">
      <div className="flex min-w-0 flex-1 flex-col">
        <span>{label}</span>
        {hint && <span className="text-[11px] text-ink-2">{hint}</span>}
      </div>
      <div className="flex shrink-0 items-center gap-2">{children}</div>
    </div>
  );
}

/** A macOS segmented control. */
export function Segmented<T extends string>({
  value,
  options,
  onChange,
}: {
  value: T;
  options: [T, string][];
  onChange: (value: T) => void;
}) {
  return (
    <div className="inline-flex h-7 rounded-md bg-ink/[0.06] p-0.5">
      {options.map(([key, label]) => (
        <button
          key={key}
          type="button"
          className={`rounded-[5px] px-2.5 text-[12px] font-medium transition ${
            key === value ? "bg-control shadow-[0_0.5px_2px_rgb(0_0_0/0.18)]" : "text-ink-2 hover:text-ink"
          }`}
          onClick={() => onChange(key)}
        >
          {label}
        </button>
      ))}
    </div>
  );
}

/** A problem with a field, in red under it. */
export function Problem({ children }: { children: ReactNode }) {
  return (
    <p className="flex items-center gap-1 text-[12px] text-danger">
      <svg viewBox="0 0 16 16" className="size-3.5 shrink-0 fill-current" aria-hidden>
        <path d="M8 1.5a6.5 6.5 0 1 0 0 13 6.5 6.5 0 0 0 0-13ZM7.25 4.5h1.5v4.5h-1.5Zm0 6h1.5V12h-1.5Z" />
      </svg>
      {children}
    </p>
  );
}

/** mousewand's wand and stars, in the text color. */
export function Logo({ className }: { className?: string }) {
  return (
    <svg viewBox="0 0 24 24" className={`fill-current ${className ?? ""}`} aria-hidden>
      <g transform="translate(0.04 0.94)">
        <g transform="translate(4.2 20.8) rotate(-47.45)">
          <path d="M15.42 -1.4H0.77A0.77 0.77 0 0 0 0 -0.63V0.63A0.77 0.77 0 0 0 0.77 1.4H15.42Z" />
          <rect x="14.97" y="-0.95" width="4.4" height="1.9" rx="0.32" className="fill-none stroke-current" strokeWidth={0.9} />
        </g>
        <path d="M20.4 0.7C20.77 2.63 20.77 2.63 22.7 3C20.77 3.37 20.77 3.37 20.4 5.3C20.03 3.37 20.03 3.37 18.1 3C20.03 2.63 20.03 2.63 20.4 0.7Z" />
        <path d="M20.6 10.3C20.87 11.73 20.87 11.73 22.3 12C20.87 12.27 20.87 12.27 20.6 13.7C20.33 12.27 20.33 12.27 18.9 12C20.33 11.73 20.33 11.73 20.6 10.3Z" />
        <path d="M14.4 1.1C14.61 2.19 14.61 2.19 15.7 2.4C14.61 2.61 14.61 2.61 14.4 3.7C14.19 2.61 14.19 2.61 13.1 2.4C14.19 2.19 14.19 2.19 14.4 1.1Z" />
      </g>
    </svg>
  );
}

/** A shape's outline, fitted into a small square. */
export function ShapeThumb({ outline, className }: { outline: [number, number][]; className?: string }) {
  if (outline.length === 0) return null;
  const xs = outline.map((p) => p[0]);
  const ys = outline.map((p) => p[1]);
  const [x0, y0] = [Math.min(...xs), Math.min(...ys)];
  const size = Math.max(Math.max(...xs) - x0, Math.max(...ys) - y0) || 1;
  const pad = size * 0.12;
  const [cx, cy] = [(Math.max(...xs) + x0) / 2, (Math.max(...ys) + y0) / 2];
  return (
    <svg
      viewBox={`${cx - size / 2 - pad} ${cy - size / 2 - pad} ${size + 2 * pad} ${size + 2 * pad}`}
      className={className}
      aria-hidden
    >
      <polyline
        points={outline.join(" ")}
        className="fill-none stroke-current"
        strokeWidth={size * 0.07}
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  );
}
