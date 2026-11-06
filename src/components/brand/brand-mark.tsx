import { cn } from "@/lib/utils";

/** Liemo's mark: a faceted L. Takes the current text colour. */
export function BrandMark({ className }: { className?: string }) {
  return (
    <svg viewBox="258 163 237 237" fill="currentColor" aria-hidden className={cn("size-[22px] shrink-0", className)}>
      <path d="M371 163 400 242 325 400 296 321Z" />
      <path d="M325 400 428 321 457 400Z" />
    </svg>
  );
}
