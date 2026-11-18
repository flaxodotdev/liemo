// The Liemo mark (same paths as the app icon) beside the wordmark.
export function Logo({ className = "" }: { className?: string }) {
  return (
    <span className={`inline-flex items-center gap-2 ${className}`}>
      <svg viewBox="296 163 161 237" className="h-[1.1em] w-auto fill-current" aria-hidden>
        <path d="M371 163 400 242 325 400 296 321Z" />
        <path d="M325 400 428 321 457 400Z" />
      </svg>
      <span className="font-serif text-[1.3em] leading-none tracking-[-0.01em]">Liemo</span>
    </span>
  );
}
