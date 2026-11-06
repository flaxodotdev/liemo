"use client";

import * as React from "react";
import Link from "next/link";
import { useRouter } from "next/navigation";
import { Clapperboard, Copy, Film, Loader2, MoreHorizontal, Pencil, Plus, Search, Sparkles, Trash2, Upload } from "lucide-react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuSeparator, DropdownMenuTrigger } from "@/components/ui/dropdown-menu";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import {
  AlertDialog, AlertDialogAction, AlertDialogCancel, AlertDialogContent, AlertDialogDescription,
  AlertDialogFooter, AlertDialogHeader, AlertDialogTitle,
} from "@/components/ui/alert-dialog";
import { deleteProject, duplicateProject, listProjects, renameProject, type ProjectSummary } from "@/lib/projects/db";
import { createProject, relativeTime, setPendingImport, type Template } from "@/lib/projects/session";
import { tc } from "@/lib/editor/time";
import { assetThumb } from "@/lib/editor/render";
import { BrandMark } from "@/components/brand/brand-mark";

type Sort = "edited" | "created" | "name";

export function Home() {
  const router = useRouter();
  const [projects, setProjects] = React.useState<ProjectSummary[] | null>(null);
  const [failed, setFailed] = React.useState(false);
  const [query, setQuery] = React.useState("");
  const [sort, setSort] = React.useState<Sort>("edited");
  const [busy, setBusy] = React.useState<Template | "video" | null>(null);
  const [renaming, setRenaming] = React.useState<ProjectSummary | null>(null);
  const [deleting, setDeleting] = React.useState<ProjectSummary | null>(null);
  const fileInput = React.useRef<HTMLInputElement>(null);

  const refresh = React.useCallback(() => {
    listProjects().then((p) => { setProjects(p); setFailed(false); }).catch(() => setFailed(true));
  }, []);
  React.useEffect(() => {
    refresh();
    try { const s = localStorage.getItem("rayna-sort") as Sort | null; if (s) setSort(s); } catch {}
  }, [refresh]);

  const start = async (template: Template, files?: FileList) => {
    setBusy(files ? "video" : template);
    try {
      const name = files?.[0] ? files[0].name.replace(/\.[^.]+$/, "") : undefined;
      const id = await createProject(template, name);
      if (files?.length) setPendingImport(files);
      router.push(`/p/${id}`);
    } catch {
      toast("Couldn't create a project. Project storage isn't available in this browser window.");
      setBusy(null);
    }
  };

  const shown = React.useMemo(() => {
    const q = query.trim().toLowerCase();
    const list = (projects ?? []).filter((p) => !q || p.name.toLowerCase().includes(q));
    return list.sort((a, b) => sort === "name" ? a.name.localeCompare(b.name) : sort === "created" ? b.createdAt - a.createdAt : b.updatedAt - a.updatedAt);
  }, [projects, query, sort]);

  return (
    <div className="min-h-dvh">
      <header className="sticky top-0 z-10 border-b border-hairline-soft bg-canvas/85 backdrop-blur">
        <div className="mx-auto flex h-14 max-w-[1200px] items-center gap-3 px-4 sm:px-8">
          <Link href="/" className="flex items-center gap-2 font-display text-[22px] font-semibold tracking-[-1px]">
            <BrandMark />Liemo
          </Link>
          <div className="flex-1" />
          <Button size="lg" className="px-4" disabled={!!busy} onClick={() => start("blank")}>
            {busy === "blank" ? <Loader2 className="animate-spin" /> : <Plus />}New project
          </Button>
        </div>
      </header>

      <main className="mx-auto flex max-w-[1200px] flex-col gap-12 px-4 pt-12 pb-24 sm:px-8 sm:pt-16">
        <section className="flex flex-col gap-3">
          <h1 className="font-display text-[44px] leading-none font-semibold tracking-[-0.05em] text-balance sm:text-[62px]">Projects</h1>
          <p className="type-body text-ink-muted">Every edit saves as you work. Projects stay in this browser, including the media you import.</p>
        </section>

        <section aria-label="Start something new" className="grid gap-3 sm:grid-cols-3">
          <StartCard title="Blank project" body="An empty timeline with titles, overlays, video and two audio tracks."
            icon={busy === "blank" ? <Loader2 className="animate-spin" /> : <Plus />} disabled={!!busy} onClick={() => start("blank")} />
          <StartCard title="Start from the sample" art={<SampleStrip />} body="The Lisbon summer reel: footage, titles, music and effects, ready to remix."
            icon={busy === "sample" ? <Loader2 className="animate-spin" /> : <Sparkles />} disabled={!!busy} onClick={() => start("sample")} />
          <StartCard title="Start from a video" body="Pick video, photo or audio files. They go straight onto a new timeline."
            icon={busy === "video" ? <Loader2 className="animate-spin" /> : <Upload />} disabled={!!busy} onClick={() => fileInput.current?.click()} />
          <input ref={fileInput} type="file" accept="video/*,audio/*,image/*" multiple hidden
            onChange={(e) => { if (e.target.files?.length) void start("blank", e.target.files); }} />
        </section>

        <section aria-labelledby="your-projects" className="flex flex-col gap-5">
          <div className="flex flex-wrap items-center gap-3">
            <h2 id="your-projects" className="type-headline">Your projects</h2>
            {projects && <span className="type-timecode text-[12px] text-ink-muted">{projects.length}</span>}
            <div className="flex-1" />
            <div className="relative w-full sm:w-60">
              <Search className="pointer-events-none absolute top-1/2 left-3 size-4 -translate-y-1/2 text-ink-muted" />
              <Input id="project-search" value={query} onChange={(e) => setQuery(e.target.value)} placeholder="Search projects" aria-label="Search projects"
                className="h-9 rounded-full bg-surface-1 pl-9 type-caption" />
            </div>
            <Select value={sort} onValueChange={(v) => { setSort(v as Sort); try { localStorage.setItem("rayna-sort", v); } catch {} }}>
              <SelectTrigger aria-label="Sort projects" className="h-9! rounded-full bg-surface-1 px-4 type-caption"><SelectValue /></SelectTrigger>
              <SelectContent>
                <SelectItem value="edited">Last edited</SelectItem>
                <SelectItem value="created">Newest</SelectItem>
                <SelectItem value="name">Name</SelectItem>
              </SelectContent>
            </Select>
          </div>

          {failed ? (
            <Empty title="Projects can't be stored here" body="This browser window blocks local storage, which private browsing and some privacy settings do. Open Liemo in a normal window to keep projects." />
          ) : projects === null ? (
            <div className="grid grid-cols-[repeat(auto-fill,minmax(250px,1fr))] gap-x-4 gap-y-6" aria-busy>
              {[0, 1, 2].map((i) => <div key={i} className="aspect-video animate-pulse rounded-lg bg-surface-1" />)}
            </div>
          ) : projects.length === 0 ? (
            <Empty title="No projects yet" body="Start one above. The sample is the quickest way to see what Liemo can do." />
          ) : shown.length === 0 ? (
            <Empty title={`Nothing matches “${query.trim()}”`} body="Try a different name, or clear the search." />
          ) : (
            <ul className="grid grid-cols-[repeat(auto-fill,minmax(250px,1fr))] gap-x-4 gap-y-6">
              {shown.map((p) => (
                <ProjectCard key={p.id} project={p}
                  onRename={() => setRenaming(p)}
                  onDelete={() => setDeleting(p)}
                  onDuplicate={async () => {
                    const id = await duplicateProject(p.id).catch(() => null);
                    if (id) { toast(`Duplicated ${p.name}`); refresh(); } else toast("Couldn't duplicate that project");
                  }} />
              ))}
            </ul>
          )}
        </section>
      </main>

      <RenameDialog project={renaming} onClose={() => setRenaming(null)}
        onSave={async (name) => { if (!renaming) return; await renameProject(renaming.id, name); setRenaming(null); refresh(); toast("Renamed"); }} />

      <AlertDialog open={!!deleting} onOpenChange={(o) => !o && setDeleting(null)}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Delete “{deleting?.name}”?</AlertDialogTitle>
            <AlertDialogDescription>The project and any media only it uses are removed from this browser. This can&apos;t be undone.</AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>Keep project</AlertDialogCancel>
            <AlertDialogAction variant="destructive" onClick={async () => {
              if (!deleting) return;
              const name = deleting.name;
              await deleteProject(deleting.id).catch(() => toast("Couldn't delete that project"));
              setDeleting(null); refresh(); toast(`Deleted ${name}`);
            }}>Delete project</AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </div>
  );
}

function StartCard({ title, body, icon, onClick, disabled, art }: {
  title: string; body: string; icon: React.ReactNode; onClick: () => void; disabled?: boolean; art?: React.ReactNode;
}) {
  return (
    <button type="button" onClick={onClick} disabled={disabled}
      className="group relative flex min-h-40 flex-col justify-between gap-6 overflow-hidden rounded-xl bg-surface-1 p-5 text-left transition-colors outline-none focus-visible:ring-2 focus-visible:ring-ring enabled:hover:bg-surface-2 disabled:opacity-60">
      <span className="flex items-center justify-between gap-3">
        <span className="grid size-10 shrink-0 place-items-center rounded-full bg-surface-3 [&_svg]:size-4">{icon}</span>
        {art}
      </span>
      <span className="flex flex-col gap-1.5">
        <span className="type-headline">{title}</span>
        <span className="type-caption font-normal text-ink-muted">{body}</span>
      </span>
    </button>
  );
}

function ProjectCard({ project: p, onRename, onDuplicate, onDelete }: { project: ProjectSummary; onRename: () => void; onDuplicate: () => void; onDelete: () => void }) {
  const href = `/p/${p.id}`;
  return (
    <li className="group relative flex flex-col gap-3">
      <Link href={href} className="relative block aspect-video overflow-hidden rounded-lg bg-surface-1 outline-none ring-signal transition-shadow focus-visible:ring-2 group-hover:shadow-[0_0_0_1px_var(--hairline),0_10px_30px_rgb(0_0_0/.35)]">
        {p.thumb
          // eslint-disable-next-line @next/next/no-img-element -- local data-URL thumbnail
          ? <img src={p.thumb} alt="" className="size-full object-cover transition-transform duration-300 group-hover:scale-[1.02]" />
          : <span className="grid size-full place-items-center text-ink-muted"><Clapperboard className="size-6" /></span>}
        <span className="absolute right-2 bottom-2 rounded-xs bg-black/65 px-1.5 py-0.5 type-timecode text-[11px]">{tc(p.duration)}</span>
        <span className="sr-only">Open {p.name}</span>
      </Link>
      <div className="flex items-start gap-2">
        <div className="flex min-w-0 flex-1 flex-col gap-0.5">
          <Link href={href} className="truncate type-body-sm hover:underline focus-visible:underline focus-visible:outline-none">{p.name}</Link>
          <span className="type-micro text-ink-muted">
            Edited {relativeTime(p.updatedAt)} · {p.clipCount} {p.clipCount === 1 ? "clip" : "clips"}
          </span>
        </div>
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <Button size="icon-sm" variant="ghost" aria-label={`More for ${p.name}`}><MoreHorizontal /></Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="end" className="w-44">
            <DropdownMenuItem asChild><Link href={href}><Film />Open</Link></DropdownMenuItem>
            <DropdownMenuItem onSelect={onRename}><Pencil />Rename</DropdownMenuItem>
            <DropdownMenuItem onSelect={onDuplicate}><Copy />Duplicate</DropdownMenuItem>
            <DropdownMenuSeparator />
            <DropdownMenuItem variant="destructive" onSelect={onDelete}><Trash2 />Delete</DropdownMenuItem>
          </DropdownMenuContent>
        </DropdownMenu>
      </div>
    </li>
  );
}

function RenameDialog({ project, onClose, onSave }: { project: ProjectSummary | null; onClose: () => void; onSave: (name: string) => void }) {
  const [name, setName] = React.useState("");
  React.useEffect(() => { if (project) setName(project.name); }, [project]);
  return (
    <Dialog open={!!project} onOpenChange={(o) => !o && onClose()}>
      <DialogContent>
        <form className="flex flex-col gap-4" onSubmit={(e) => { e.preventDefault(); if (name.trim()) onSave(name.trim()); }}>
          <DialogHeader>
            <DialogTitle>Rename project</DialogTitle>
            <DialogDescription>Shown on your projects page and in the editor.</DialogDescription>
          </DialogHeader>
          <Input id="rename-project" aria-label="Project name" value={name} onChange={(e) => setName(e.target.value)} autoFocus maxLength={80} />
          <DialogFooter>
            <Button type="button" variant="secondary" onClick={onClose}>Cancel</Button>
            <Button type="submit" disabled={!name.trim()}>Save name</Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}

function Empty({ title, body }: { title: string; body: string }) {
  return (
    <div className="flex flex-col items-center gap-2 rounded-xl border border-dashed border-hairline px-6 py-16 text-center">
      <p className="type-headline">{title}</p>
      <p className="max-w-md type-caption font-normal text-ink-muted">{body}</p>
    </div>
  );
}

/** Three frames from the sample's footage, so the card shows what you get. */
function SampleStrip() {
  const [thumbs, setThumbs] = React.useState<string[]>([]);
  React.useEffect(() => { setThumbs(["sc-coast", "sc-drive", "sc-aurora"].map((id) => assetThumb(id) ?? "")); }, []);
  return (
    <span className="flex gap-1" aria-hidden>
      {thumbs.map((t, i) => (
        <span key={i} className="h-10 w-[60px] rounded-sm bg-surface-3 bg-cover bg-center" style={t ? { backgroundImage: `url(${t})` } : undefined} />
      ))}
    </span>
  );
}
