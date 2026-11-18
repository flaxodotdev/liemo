import type { Metadata } from "next";
import { Logo } from "@/components/logo";
import { REPO } from "@/lib/release";

// Mirrors PRIVACY.md at the repo root; keep the two in step.

export const metadata: Metadata = { title: "Privacy · Liemo" };

export default function Privacy() {
  return (
    <div className="mx-auto max-w-[680px] px-4 py-10 sm:px-6">
      <a href="/" aria-label="Liemo home" className="text-[17px]">
        <Logo />
      </a>

      <article className="mt-16 pb-24 text-[16px] leading-relaxed text-muted [&_a]:text-ink [&_a]:underline [&_a]:decoration-line-strong [&_a]:underline-offset-4 [&_a:hover]:underline [&_h2]:type-title [&_h2]:mb-3 [&_h2]:mt-12 [&_h2]:text-ink [&_li]:mt-3 [&_strong]:font-medium [&_strong]:text-ink">
        <h1 className="type-section text-ink">Privacy</h1>
        <p className="mt-6">
          Liemo is a video editor for Mac, iPhone and iPad. It has no accounts, no analytics, no ads and no tracking,
          and there is no Liemo server: your work stays on your device.
        </p>

        <h2>What stays on your device</h2>
        <ul>
          <li>
            <strong>Projects, imported photos, videos and audio, recordings and exports.</strong> Liemo keeps them in
            its own storage on your device. Nothing is uploaded.
          </li>
          <li>
            <strong>Photos.</strong> Liemo only sees the photos and videos you pick. When you export, the finished
            video is saved to your photo library, with your permission.
          </li>
          <li>
            <strong>Microphone.</strong> Used only while you record a voiceover. The recording is saved in your
            project.
          </li>
        </ul>

        <h2>When something leaves your device</h2>
        <ul>
          <li>
            <strong>Auto captions (iPhone and iPad)</strong> use Apple&apos;s speech recognition. On devices that
            support it, this runs entirely on the device; otherwise the audio is processed by Apple under{" "}
            <a href="https://www.apple.com/legal/privacy/">Apple&apos;s privacy policy</a>.
          </li>
          <li>
            <strong>Liemo AI is optional.</strong> On iPhone and iPad it works only after you add your own Anthropic
            API key, which is kept in your device&apos;s Keychain. When you ask it for an edit, your message and the
            details of the open project (clip names, timings, text, and frames of the video when it checks its work)
            are sent to Anthropic&apos;s API under your account, governed by{" "}
            <a href="https://www.anthropic.com/legal/privacy">Anthropic&apos;s privacy policy</a>. On the Mac, Liemo
            AI uses the AI command-line tools you&apos;ve installed and signed in to yourself, under their own terms.
          </li>
          <li>
            <strong>Updates (Mac).</strong> The Mac app checks GitHub for new versions of Liemo.
          </li>
        </ul>

        <h2>Contact</h2>
        <p>
          Questions: open an issue at <a href={`${REPO}/issues`}>github.com/flaxodotdev/liemo</a>.
        </p>
      </article>
    </div>
  );
}
