import type { Metadata } from "next";
import { Inter, Source_Serif_4, JetBrains_Mono } from "next/font/google";
import "./globals.css";

const inter = Inter({ variable: "--font-inter", subsets: ["latin"] });
const serif = Source_Serif_4({ variable: "--font-source-serif", subsets: ["latin"], axes: ["opsz"] });
const jetbrains = JetBrains_Mono({ variable: "--font-jetbrains", subsets: ["latin"], weight: ["400", "500"] });

export const metadata: Metadata = {
  metadataBase: new URL("https://liemo.vercel.app"),
  title: "Liemo: the video editor for the AI era",
  description:
    "A fast, native, open-source video editor. Edit yourself, or let Claude Code, Codex, Cursor or any MCP client edit with you, live on your timeline.",
  openGraph: {
    title: "Liemo: the video editor for the AI era",
    description: "A fast, native, open-source video editor that AI agents can edit with you.",
    url: "/",
    siteName: "Liemo",
    type: "website",
  },
  twitter: { card: "summary_large_image" },
};

const THEME_SCRIPT = `try{var t=localStorage.getItem("theme");if(t==="light"||t==="dark")document.documentElement.dataset.theme=t}catch(e){}`;

export default function RootLayout({ children }: Readonly<{ children: React.ReactNode }>) {
  return (
    <html lang="en" className={`${inter.variable} ${serif.variable} ${jetbrains.variable}`} suppressHydrationWarning>
      <head>
        {/* Apply a saved theme before first paint, so dark mode never flashes white. */}
        <script dangerouslySetInnerHTML={{ __html: THEME_SCRIPT }} />
      </head>
      <body>{children}</body>
    </html>
  );
}
