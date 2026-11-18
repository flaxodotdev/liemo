"use client";

import { useEffect, useState } from "react";
import { Download as DownloadIcon } from "lucide-react";
import type { Download, Platform } from "@/lib/release";

// Browsers don't say whether a Mac is Apple silicon, so Macs get the arm64 build and the
// Intel one is linked underneath. Windows and Linux visitors get their own build first.
function detect(): Platform {
  const ua = navigator.userAgent;
  if (/Windows/i.test(ua)) return "windows";
  if (/Linux/i.test(ua) && !/Android/i.test(ua)) return "linux";
  return "mac-arm";
}

export function DownloadButton({
  downloads,
  version,
  children,
}: {
  downloads: Download[];
  version: string | null;
  children?: React.ReactNode;
}) {
  const [platform, setPlatform] = useState<Platform>("mac-arm");
  useEffect(() => setPlatform(detect()), []);

  const primary = downloads.find((d) => d.platform === platform) ?? downloads[0];
  const intel = downloads.find((d) => d.platform === "mac-intel");

  return (
    <div className="flex flex-col items-center gap-4">
      <div className="flex flex-wrap items-center justify-center gap-2.5">
        <a href={primary.url} className="pill pill-dark">
          <DownloadIcon className="size-4" strokeWidth={2} />
          Download for {primary.label}
        </a>
        {children}
      </div>
      <p className="text-[12.5px] text-muted">
        {version ? `Version ${version}` : "Latest version"}
        {platform === "mac-arm" && intel && (
          <>
            {" · "}
            <a href={intel.url} className="underline decoration-line-strong underline-offset-4 hover:text-ink hover:decoration-ink">
              Intel Mac
            </a>
          </>
        )}
        {" · "}
        <a href="#download" className="underline decoration-line-strong underline-offset-4 hover:text-ink hover:decoration-ink">
          Other platforms
        </a>
      </p>
    </div>
  );
}
