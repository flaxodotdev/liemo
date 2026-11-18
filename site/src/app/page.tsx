import { ArrowUpRight, Check } from "lucide-react";
import { CopyButton } from "@/components/copy-button";
import { DownloadButton } from "@/components/download-button";
import { HalftoneReel } from "@/components/halftone-reel";
import { Logo } from "@/components/logo";
import { ThemeToggle } from "@/components/theme-toggle";
import { latestRelease, REPO } from "@/lib/release";

const BINARY = "/Applications/Liemo.app/Contents/MacOS/Liemo";
const CLAUDE_CODE = `claude mcp add liemo -- ${BINARY} mcp`;
const MCP_JSON = `{
  "mcpServers": {
    "liemo": { "command": "${BINARY}", "args": ["mcp"] }
  }
}`;

// The CLIs Liemo AI can drive (native/src/agent.rs).
const AGENTS = ["Claude Code", "Codex", "Gemini CLI", "Cursor", "GitHub Copilot", "OpenCode", "Qwen Code", "Grok", "Amp"];

// A session as an agent would run it over MCP: real tool names and asset, effect and transition ids.
const SESSION: { tool: string; args: string }[] = [
  { tool: "create_project", args: `format: "reel", name: "Summer, slowly"` },
  { tool: "add_clip", args: `asset: "sc-coast", duration: 8` },
  { tool: "add_clip", args: `asset: "sc-surf", start: 8, duration: 7` },
  { tool: "set_transition", args: `transition: "dissolve", duration: 1` },
  { tool: "apply_effect", args: `effect: "ig-golden", amount: 0.8` },
  { tool: "add_text", args: `style: "tx-title", content: "Summer, slowly"` },
  { tool: "add_clip", args: `asset: "mu-golden"` },
  { tool: "render_frame", args: `time: 1.5` },
];

const CAN_DO = [
  ["Projects", "List, create in any format, open in the editor."],
  ["Timeline", "Add footage, music, overlays and text. Move, trim, split, duplicate."],
  ["Look", "100+ effects and filters, transitions, fades, transforms."],
  ["Check", "Render a frame as an image to see the result."],
  ["Deliver", "Export MP4, WebM, GIF, stills or audio."],
];

const FEATURES = [
  {
    title: "A frame-accurate timeline",
    body: "Titles, overlays, main video, music and voice tracks in HH:MM:SS:FF. Insert and overwrite, snapping, split, and undo for everything.",
  },
  {
    title: "100+ looks",
    body: "Instagram-style filters, film and mood grades, and shader filters like VHS and ASCII that run on the GPU. Stack them; set how strong each one is.",
  },
  {
    title: "Transitions on the GPU",
    body: "Fifty of them: dissolves, pushes, dips, Thirds and shader transitions, blended right across the cut.",
  },
  {
    title: "Remove the background",
    body: "Cut out the subject with Apple's Vision on Mac, iPhone and iPad, then feather, outline or blur what's behind. Chroma key works everywhere.",
  },
  {
    title: "Titles, even in 3D",
    body: "Title, lower third, quote and poster styles, stacked on one track. Tilt text in perspective, outline it, or let it invert what's under it.",
  },
  {
    title: "Every shape, every export",
    body: "YouTube, Reels, TikTok, square or any custom size at 24 to 60 fps. Out as MP4, WebM, GIF, stills, WAV or M4A.",
  },
];

const PRIVATE = [
  ["No account, no server", "There's no Liemo server and nothing to sign up for. No analytics, no ads, no tracking."],
  ["Your files stay put", "Footage is used from where it is on your disk. Nothing is uploaded; exports land in Downloads."],
  ["Open source, MIT", "Read it, build it, change it. The editor, its renderer and the MCP server are all on GitHub."],
];

export default async function Home() {
  const { version, downloads } = await latestRelease();

  return (
    <div className="min-h-dvh overflow-x-clip">
      <Nav />

      {/* The frame: hairlines down both sides, sections ruled across. */}
      <div className="mx-auto max-w-[1240px] border-line sm:border-x">
        <main>
          {/* Hero */}
          <section className="px-4 pb-16 pt-16 text-center sm:px-10 sm:pt-24">
            <a
              href={`${REPO}/releases/latest`}
              className="inline-flex items-center gap-2 rounded-full border border-line px-3 py-1 text-[12.5px] text-muted transition-colors hover:border-line-strong hover:text-ink"
            >
              <span className="size-1.5 rounded-full bg-live" />
              {version ? `New in ${version}` : "New"}: background removal and 3D text
              <ArrowUpRight className="size-3" />
            </a>
            <h1 className="type-hero mx-auto mt-8 max-w-[16ch] text-balance">
              The video editor for the <span className="text-muted">AI</span> era
            </h1>
            <p className="mx-auto mt-6 max-w-[30rem] text-pretty text-[15px] leading-relaxed text-muted">
              A fast, native editor you drive yourself, or hand to Claude Code, Codex, Cursor or any MCP client.
              Their edits land live on your timeline, and every one can be undone.
            </p>
            <div className="mt-9">
              <DownloadButton downloads={downloads} version={version}>
                <a href={REPO} className="pill pill-outline">
                  View on GitHub
                </a>
              </DownloadButton>
            </div>

            <div id="reel" className="mt-12 scroll-mt-16 sm:mt-14">
              <HalftoneReel />
            </div>
          </section>

          {/* AI editing */}
          <section id="ai" className="scroll-mt-16 border-t border-line">
            <SectionLabel n="01" name="AI editing" />
            <div className="grid lg:grid-cols-2">
              <div className="px-4 pb-12 pt-10 sm:px-10 lg:pb-16">
                <h2 className="type-section max-w-[13ch] text-balance">
                  Your agent, <span className="text-muted">on your timeline</span>
                </h2>
                <p className="type-lede mt-6 max-w-[30rem] text-pretty">
                  Liemo has an MCP server built in. Ask for a cut in plain words and the agent builds it from the same
                  tools you use: clips, effects, transitions and titles. Then it renders a frame to check its own work.
                </p>
                <dl className="mt-10 border-t border-line">
                  {CAN_DO.map(([k, v]) => (
                    <div key={k} className="grid grid-cols-[6.5rem_1fr] gap-4 border-b border-line py-3.5 text-[14px]">
                      <dt className="font-mono text-[11px] uppercase leading-[21px] tracking-[0.1em] text-muted">{k}</dt>
                      <dd>{v}</dd>
                    </div>
                  ))}
                </dl>
              </div>
              <div className="border-line bg-paper-2 px-4 py-10 sm:px-10 lg:border-l lg:py-16">
                <AgentSession />
              </div>
            </div>

            {/* Two ways in */}
            <div className="grid border-t border-line md:grid-cols-2">
              <div className="px-4 py-10 sm:px-10 sm:py-12">
                <p className="type-label">Inside the editor</p>
                <h3 className="type-title mt-3">Liemo AI</h3>
                <p className="mt-3 max-w-[28rem] text-[14.5px] leading-relaxed text-muted">
                  Open the AI sidebar and chat. It uses the coding agent you already have installed and signed in to,
                  so there&apos;s no new account and no key.
                </p>
                <ul className="mt-6 flex flex-wrap gap-1.5">
                  {AGENTS.map((a) => (
                    <li key={a} className="rounded-full border border-line px-3 py-1 text-[12.5px]">
                      {a}
                    </li>
                  ))}
                </ul>
              </div>
              <div className="border-line px-4 py-10 sm:px-10 sm:py-12 max-md:border-t md:border-l">
                <p className="type-label">From any MCP client</p>
                <h3 className="type-title mt-3">Connect from outside</h3>
                <p className="mt-3 max-w-[28rem] text-[14.5px] leading-relaxed text-muted">
                  While Liemo is open, edits appear in it live. When it&apos;s closed, the agent edits the project files
                  directly.
                </p>
                <Snippet label="Claude Code" code={CLAUDE_CODE} />
                <Snippet label="Claude Desktop, Cursor and others" code={MCP_JSON} />
              </div>
            </div>
          </section>

          {/* The editor */}
          <section id="editor" className="scroll-mt-16 border-t border-line">
            <SectionLabel n="02" name="The editor" />
            <div className="px-4 pb-12 pt-10 text-center sm:px-10 sm:pb-16">
              <h2 className="type-section mx-auto max-w-[16ch] text-balance">
                A real editor <span className="text-muted">first</span>
              </h2>
              <p className="type-lede mx-auto mt-6 max-w-[32rem] text-pretty">
                The AI is optional. Everything it can do, you can do by hand, at full speed, with no network
                connection. Native, GPU-accelerated, written in Rust.
              </p>
              <figure className="mt-12 sm:mt-14">
                {/* eslint-disable-next-line @next/next/no-img-element */}
                <img
                  src="/editor.webp"
                  alt="The Liemo editor: a coastal shot titled “Summer, slowly” in the preview, the inspector on the right, and titles, overlays, video and music tracks on the timeline."
                  width={2880}
                  height={1728}
                  loading="lazy"
                  className="block h-auto w-full rounded-[10px] shadow-[0_1px_2px_rgba(0,0,0,0.06),0_30px_80px_-30px_rgba(0,0,0,0.35)] ring-1 ring-frame-ring sm:rounded-[14px]"
                />
                <figcaption className="type-label mt-4">Liemo 0.4 on macOS</figcaption>
              </figure>
            </div>

            <div className="grid gap-px border-t border-line bg-line sm:grid-cols-2 lg:grid-cols-3">
              {FEATURES.map((f, i) => (
                <div key={f.title} className="bg-paper px-4 py-10 sm:px-10">
                  <p className="type-label">{String(i + 1).padStart(2, "0")}</p>
                  <h3 className="type-title mt-6">{f.title}</h3>
                  <p className="mt-3 text-[14.5px] leading-relaxed text-muted">{f.body}</p>
                </div>
              ))}
            </div>
          </section>

          {/* Private by design */}
          <section className="border-t border-line">
            <SectionLabel n="03" name="Yours" />
            <div className="grid gap-px bg-line md:grid-cols-3">
              {PRIVATE.map(([t, b]) => (
                <div key={t} className="bg-paper px-4 pb-12 pt-10 sm:px-10">
                  <h3 className="type-title">{t}</h3>
                  <p className="mt-3 text-[14.5px] leading-relaxed text-muted">{b}</p>
                </div>
              ))}
            </div>
          </section>

          {/* Download */}
          <section id="download" className="scroll-mt-16 border-t border-line">
            <SectionLabel n="04" name="Download" />
            <div className="px-4 pb-20 pt-12 text-center sm:px-10 sm:pb-28">
              <h2 className="type-section mx-auto max-w-[14ch] text-balance">
                Edit your next video <span className="text-muted">with Liemo</span>
              </h2>
              <p className="type-lede mx-auto mt-6 max-w-[30rem]">
                Free and open source{version ? `, version ${version}` : ""}. Signed and notarised on the Mac, where it
                keeps itself up to date.
              </p>

              <ul className="mx-auto mt-12 max-w-[640px] border-t border-line text-left">
                {downloads.map((d) => (
                  <li key={d.platform} className="border-b border-line">
                    <a href={d.url} className="group flex items-center justify-between gap-4 py-4">
                      <span className="flex flex-wrap items-baseline gap-x-3">
                        <span className="type-title text-[1.25rem]">{d.label}</span>
                        <span className="font-mono text-[11px] uppercase tracking-[0.08em] text-muted">{d.detail}</span>
                      </span>
                      <span className="pill pill-outline h-8 px-3.5 text-[12.5px] group-hover:border-ink">Download</span>
                    </a>
                  </li>
                ))}
                <li className="flex items-center justify-between gap-4 border-b border-line py-4">
                  <span className="flex flex-wrap items-baseline gap-x-3">
                    <span className="type-title text-[1.25rem]">iPhone &amp; iPad</span>
                    <span className="font-mono text-[11px] uppercase tracking-[0.08em] text-muted">App Store</span>
                  </span>
                  <span className="text-[12.5px] text-muted">Coming soon</span>
                </li>
              </ul>
              <p className="mx-auto mt-4 max-w-[640px] text-left text-[12.5px] text-muted">
                Linux needs FFmpeg 6 or newer and a Vulkan driver. Windows includes FFmpeg.
              </p>
            </div>
          </section>
        </main>

        <Footer />
      </div>
    </div>
  );
}

function Nav() {
  return (
    <header className="sticky top-0 z-50 border-b border-line bg-paper/85 backdrop-blur-md">
      <nav className="mx-auto flex h-16 max-w-[1240px] items-center justify-between border-line px-4 sm:border-x sm:px-10">
        <div className="flex items-center gap-10">
          <a href="#" aria-label="Liemo home" className="text-[17px]">
            <Logo />
          </a>
          <div className="hidden items-center gap-7 text-[14px] md:flex">
            <a href="#ai" className="hover:text-muted">AI editing</a>
            <a href="#editor" className="hover:text-muted">Editor</a>
            <a href="#download" className="hover:text-muted">Download</a>
          </div>
        </div>
        <div className="flex items-center gap-2">
          <ThemeToggle />
          <a href={REPO} className="pill pill-outline hidden sm:inline-flex">
            GitHub
          </a>
          <a href="#download" className="pill pill-dark">
            Get Liemo
          </a>
        </div>
      </nav>
    </header>
  );
}

function SectionLabel({ n, name }: { n: string; name: string }) {
  return (
    <div className="flex items-center justify-between border-b border-line px-4 py-3 sm:px-10">
      <span className="type-label">{name}</span>
      <span className="type-label">{n}</span>
    </div>
  );
}

function AgentSession() {
  return (
    <div className="overflow-hidden rounded-[12px] border border-line bg-paper shadow-[0_1px_2px_rgba(0,0,0,0.04),0_20px_50px_-30px_rgba(0,0,0,0.25)]">
      <div className="flex items-center justify-between border-b border-line px-5 py-3">
        <span className="font-mono text-[11.5px] text-muted">claude · mcp: liemo</span>
        <span className="flex items-center gap-1.5 text-[11.5px] text-muted">
          <span className="size-1.5 rounded-full bg-live" />
          Liemo open
        </span>
      </div>

      <div className="space-y-5 p-5 sm:p-6">
        <p className="ml-auto max-w-[88%] rounded-[14px] rounded-br-[4px] bg-paper-2 px-4 py-3 text-[14.5px] leading-relaxed">
          Make a 15-second Reel from the coast footage. Call it “Summer, slowly”, warm the colour up and put some
          music under it.
        </p>

        <ol className="space-y-1.5 font-mono text-[12px] leading-snug">
          {SESSION.map((s, i) => (
            <li key={i} className="flex gap-2.5">
              <Check className="mt-[1px] size-3.5 shrink-0 text-live" strokeWidth={2.5} />
              <span className="min-w-0">
                <span>{s.tool}</span> <span className="break-words text-muted">{s.args}</span>
              </span>
            </li>
          ))}
        </ol>

        <div className="flex gap-4">
          <div className="relative w-[120px] shrink-0 overflow-hidden rounded-[8px] ring-1 ring-frame-ring">
            {/* eslint-disable-next-line @next/next/no-img-element */}
            <img
              src="/frame.webp"
              alt="The rendered frame: coastal cliffs, warmed with the Golden hour look, titled “Summer, slowly”."
              loading="lazy"
              className="block aspect-[9/16] w-full object-cover"
              style={{ filter: "sepia(0.2) saturate(1.25) brightness(1.05)" }}
            />
            <span className="absolute inset-x-0 top-[38%] px-2 text-center font-serif text-[18px] leading-none text-white [text-shadow:0_2px_12px_rgba(0,0,0,0.45)]">
              Summer, slowly
            </span>
            <span className="absolute bottom-1.5 left-1.5 rounded-[4px] bg-black/60 px-1 font-mono text-[9.5px] text-white">
              00:00:01:15
            </span>
          </div>
          <p className="self-end text-[14px] leading-relaxed text-muted">
            <span className="text-ink">Done: a 15-second Reel.</span> The title sits over the cliffs for three seconds,
            then dissolves into the surf with Golden hour on both shots. It&apos;s open in Liemo, and ⌘Z undoes any of
            it.
          </p>
        </div>
      </div>
    </div>
  );
}

function Snippet({ label, code }: { label: string; code: string }) {
  return (
    <div className="mt-6">
      <div className="mb-2 flex items-center justify-between gap-3">
        <span className="text-[12.5px] font-medium">{label}</span>
        <CopyButton text={code} />
      </div>
      <pre className="overflow-x-auto rounded-[8px] border border-line bg-paper-2 p-4 font-mono text-[12px] leading-relaxed">
        {code}
      </pre>
    </div>
  );
}

function Footer() {
  return (
    <footer className="border-t border-line">
      <div className="flex flex-col gap-6 px-4 py-10 text-[13px] text-muted sm:flex-row sm:items-center sm:justify-between sm:px-10">
        <div className="flex items-center gap-4">
          <Logo className="text-[15px] text-ink" />
          <span>MIT licensed</span>
        </div>
        <div className="flex flex-wrap gap-x-6 gap-y-2">
          <a href={REPO} className="hover:text-ink">GitHub</a>
          <a href={`${REPO}/releases`} className="hover:text-ink">Releases</a>
          <a href="/privacy" className="hover:text-ink">Privacy</a>
          <a href={`${REPO}/issues`} className="hover:text-ink">Report an issue</a>
        </div>
      </div>
    </footer>
  );
}
