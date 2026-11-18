// The latest GitHub release, read once at build time so the download links name real files.
// If GitHub can't be reached the links fall back to the Releases page.

export const REPO = "https://github.com/flaxodotdev/liemo";
const LATEST = `${REPO}/releases/latest`;

export type Platform = "mac-arm" | "mac-intel" | "windows" | "linux";

export type Download = { platform: Platform; label: string; detail: string; url: string };

export type Release = { version: string | null; downloads: Download[] };

const PLATFORMS: { platform: Platform; label: string; detail: string; match: RegExp }[] = [
  { platform: "mac-arm", label: "Mac", detail: "Apple silicon · .dmg", match: /macos-arm64\.dmg$/ },
  { platform: "mac-intel", label: "Mac", detail: "Intel · .dmg", match: /macos-x86_64\.dmg$/ },
  { platform: "windows", label: "Windows", detail: "x86_64 · .zip", match: /windows-x86_64\.zip$/ },
  { platform: "linux", label: "Linux", detail: "x86_64 · .tar.gz", match: /linux-x86_64\.tar\.gz$/ },
];

export async function latestRelease(): Promise<Release> {
  let tag: string | null = null;
  let assets: { name: string; browser_download_url: string }[] = [];
  try {
    const res = await fetch("https://api.github.com/repos/flaxodotdev/liemo/releases/latest", {
      headers: { Accept: "application/vnd.github+json" },
    });
    if (res.ok) {
      const json = await res.json();
      tag = json.tag_name ?? null;
      assets = json.assets ?? [];
    }
  } catch {
    // Offline build: keep the fallbacks.
  }
  return {
    version: tag?.replace(/^v/, "") ?? null,
    downloads: PLATFORMS.map(({ match, ...p }) => ({
      ...p,
      url: assets.find((a) => match.test(a.name))?.browser_download_url ?? LATEST,
    })),
  };
}
