"use client";

/**
 * avry-ui Avatar — deterministic pastel bg from the address (Skiff Facepile feel).
 * Presentational only: pass the sender address + precomputed initials.
 */
const palette: [string, string][] = [
  ["bg-orange-100", "text-orange-700"],
  ["bg-emerald-100", "text-emerald-700"],
  ["bg-sky-100", "text-sky-700"],
  ["bg-violet-100", "text-violet-700"],
  ["bg-amber-100", "text-amber-700"],
  ["bg-rose-100", "text-rose-700"],
  ["bg-teal-100", "text-teal-700"],
];

function pick(email: string): [string, string] {
  let h = 0;
  for (let i = 0; i < email.length; i++) h = (h * 31 + email.charCodeAt(i)) >>> 0;
  return palette[h % palette.length];
}

export function Avatar({
  email,
  initials,
  size = 32,
  className = "",
  src = null,
}: {
  email: string;
  initials: string;
  size?: number;
  className?: string;
  src?: string | null;
}) {
  const [bg, fg] = pick(email.toLowerCase());
  if (src) {
    return (
      // eslint-disable-next-line @next/next/no-img-element
      <img
        src={src}
        alt={email}
        width={size}
        height={size}
        style={{ width: size, height: size }}
        className={`shrink-0 rounded-full object-cover ${className}`}
      />
    );
  }
  return (
    <span
      aria-hidden
      style={{ width: size, height: size, fontSize: Math.max(10, Math.round(size * 0.36)) }}
      className={`inline-flex shrink-0 items-center justify-center rounded-full font-bold ${bg} ${fg} ${className}`}
    >
      {initials}
    </span>
  );
}

export default Avatar;
