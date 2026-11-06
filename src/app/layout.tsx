import type { Metadata } from "next";
import { Inter, Mona_Sans, JetBrains_Mono, Instrument_Serif, Bebas_Neue, Caveat } from "next/font/google";
import { TooltipProvider } from "@/components/ui/tooltip";
import { Toaster } from "@/components/ui/sonner";
import "./globals.css";

// Interface faces
const inter = Inter({ variable: "--font-inter", subsets: ["latin"] });
const mona = Mona_Sans({ variable: "--font-mona", subsets: ["latin"] });
const jetbrains = JetBrains_Mono({ variable: "--font-jetbrains", subsets: ["latin"], weight: ["400", "500", "600"] });

// Extra faces offered to text clips in the editor
const instrument = Instrument_Serif({ variable: "--font-instrument", subsets: ["latin"], weight: "400" });
const bebas = Bebas_Neue({ variable: "--font-bebas", subsets: ["latin"], weight: "400" });
const caveat = Caveat({ variable: "--font-caveat", subsets: ["latin"], weight: ["500", "700"] });

export const metadata: Metadata = {
  title: "Liemo",
  description: "A video editor with a frame-accurate timeline, media pool, music, overlays, text and effects.",
};

export default function RootLayout({ children }: Readonly<{ children: React.ReactNode }>) {
  const fonts = [inter, mona, jetbrains, instrument, bebas, caveat].map((f) => f.variable).join(" ");
  return (
    <html lang="en" className={`dark ${fonts}`}>
      <body>
        <TooltipProvider delayDuration={400}>{children}</TooltipProvider>
        <Toaster position="bottom-center" />
      </body>
    </html>
  );
}
