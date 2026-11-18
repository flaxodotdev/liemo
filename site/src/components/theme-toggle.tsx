"use client";

import { Moon, Sun } from "lucide-react";

// Flips to the opposite of what's showing and remembers it. Until clicked, the page follows
// the system setting (see the dark tokens in globals.css and the head script in layout.tsx).
export function ThemeToggle() {
  function toggle() {
    const root = document.documentElement;
    const dark = root.dataset.theme
      ? root.dataset.theme === "dark"
      : matchMedia("(prefers-color-scheme: dark)").matches;
    const next = dark ? "light" : "dark";
    root.dataset.theme = next;
    try {
      localStorage.setItem("theme", next);
    } catch {
      // Private mode: the choice lasts for this page only.
    }
  }

  return (
    <button type="button" onClick={toggle} aria-label="Switch between light and dark" className="pill pill-outline size-10 px-0">
      <Sun className="size-4 dark-only" />
      <Moon className="size-4 light-only" />
    </button>
  );
}
