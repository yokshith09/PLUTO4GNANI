import { useQuery, useQueryClient } from "@tanstack/react-query";
import { BookHeart, Check, ChevronLeft, ChevronRight, Copy, KeyRound, Lock, ListPlus } from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";
import { ConfirmAction } from "@/components/ConfirmAction";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Switch } from "@/components/ui/switch";
import { usePref } from "@/features/guide/usePref";
import { PageHeader } from "@/features/shell/PageHeader";
import { errorMessage, ipc } from "@/ipc";
import { onCommand } from "@/ipc/transport";
import { dayLabel, parseYmd, ymd } from "@/lib/dates";
import { notify } from "@/lib/notify";
import { detectTodos, titleOf } from "./detect";

export const IDLE_LOCK_MS = 5 * 60_000;
export const PICKUP_PREF = "diary.pickup";
const STATUS = ["diary", "status"] as const;

function PasswordForm({ configured, onDone }: { configured: boolean; onDone: (recoveryKey?: string) => void }) {
  const [pw, setPw] = useState("");
  const [recovering, setRecovering] = useState(false);
  const [key, setKey] = useState("");
  const [again, setAgain] = useState("");
  const [error, setError] = useState<string | null>(null);
  const qc = useQueryClient();
  const submit = async () => {
    setError(null);
    if ((!configured || recovering) && pw !== again) return setError("The two passwords are not the same.");
    try {
      let made: string | undefined;
      if (recovering) await ipc.diaryRecover(key, pw);
      else if (configured) await ipc.diaryUnlock(pw);
      else made = await ipc.diarySetup(pw);
      setPw(""); setAgain(""); setKey("");
      onDone(made);
      await qc.invalidateQueries({ queryKey: STATUS });
    } catch (e) { setError(errorMessage(e)); }
  };
  return (
    <form className="mx-auto mt-10 max-w-sm space-y-3 rounded-xl border bg-card p-6" onSubmit={(e) => { e.preventDefault(); void submit(); }}>
      <div className="flex items-center gap-2 text-sm font-semibold"><Lock aria-hidden className="size-4 text-primary" />{recovering ? "Set a new password" : configured ? "Unlock your diary" : "Set a password for your diary"}</div>
      {!configured && <p className="text-xs text-muted-foreground">Only you can read it. After you create it, Pluto shows you a recovery key once. Keep that key: it is the only way back in if you forget the password.</p>}
      {recovering && <Input autoFocus value={key} onChange={(e) => setKey(e.target.value)} aria-label="Recovery key" placeholder="ABCD-EFGH-…" autoComplete="off" />}
      <Input autoFocus={!recovering} type="password" value={pw} onChange={(e) => setPw(e.target.value)} aria-label={recovering ? "New password" : "Password"} autoComplete={configured && !recovering ? "current-password" : "new-password"} />
      {(!configured || recovering) && <Input type="password" value={again} onChange={(e) => setAgain(e.target.value)} aria-label="Repeat the password" autoComplete="new-password" />}
      {error && <p role="alert" className="text-xs text-destructive">{error}</p>}
      <Button type="submit" className="w-full" disabled={!pw || (recovering && !key.trim())}>{recovering ? "Recover diary" : configured ? "Unlock" : "Create diary"}</Button>
      {configured && (
        <p className="space-y-1 text-center text-xs text-muted-foreground">
          <button type="button" className="block w-full rounded text-primary underline outline-none focus-visible:ring-2 focus-visible:ring-ring" onClick={() => { setRecovering(!recovering); setError(null); }}>{recovering ? "Back to unlock" : "Forgot the password? Use the recovery key"}</button>
          Lost the recovery key too?{" "}
          <ConfirmAction message="Erase the whole diary? Every entry is deleted and cannot be brought back." verb="Erase" onConfirm={() => void ipc.diaryErase().then(() => qc.invalidateQueries({ queryKey: STATUS }))}>
            <button type="button" className="rounded text-primary underline outline-none focus-visible:ring-2 focus-visible:ring-ring">Erase the diary</button>
          </ConfirmAction>
        </p>
      )}
    </form>
  );
}

function RecoveryKeyCard({ recoveryKey, onDone }: { recoveryKey: string; onDone: () => void }) {
  const [saved, setSaved] = useState(false);
  const copy = async () => { try { await navigator.clipboard.writeText(recoveryKey); notify.success("Recovery key copied"); } catch { notify.error("Could not copy"); } };
  return (
    <section aria-label="Recovery key" className="mx-auto mt-10 max-w-md space-y-3 rounded-xl border bg-card p-6">
      <div className="flex items-center gap-2 text-sm font-semibold"><KeyRound aria-hidden className="size-4 text-primary" />Your recovery key</div>
      <p className="text-xs text-muted-foreground">If you forget your password, this key lets you set a new one without losing anything. Pluto shows it only now and does not keep a readable copy. Write it down or save it somewhere safe.</p>
      <p className="select-all rounded-md bg-muted px-3 py-2 text-center font-mono text-sm tracking-wider" aria-label="Recovery key value">{recoveryKey}</p>
      <Button variant="outline" size="sm" className="w-full" onClick={() => void copy()}><Copy />Copy</Button>
      <label className="flex items-center gap-2 text-xs"><input type="checkbox" className="size-4" checked={saved} onChange={(e) => setSaved(e.target.checked)} />I have saved my recovery key</label>
      <Button className="w-full" disabled={!saved} onClick={onDone}>Open my diary</Button>
    </section>
  );
}

function Editor({ onLock }: { onLock: () => void }) {
  const [day, setDay] = useState(() => ymd(new Date()));
  const [text, setText] = useState("");
  const [loaded, setLoaded] = useState(false);
  const [saved, setSaved] = useState<"idle" | "saving" | "saved">("idle");
  const [made, setMade] = useState<Record<string, string>>({});
  const pickup = usePref<boolean>(PICKUP_PREF, (r) => r === true);
  const days = useQuery({ queryKey: ["diary", "days", saved], queryFn: () => ipc.diaryDays() });
  const [query, setQuery] = useState("");
  const results = useQuery({ queryKey: ["diary", "search", query.trim()], queryFn: () => ipc.diarySearch(query), enabled: query.trim().length > 0 });
  const area = useRef<HTMLTextAreaElement>(null);
  const pending = useRef<{ day: string; text: string } | null>(null);
  const timer = useRef<ReturnType<typeof setTimeout>>();

  const flush = useCallback(async () => {
    clearTimeout(timer.current);
    const p = pending.current;
    if (!p) return;
    pending.current = null;
    setSaved("saving");
    try { await ipc.diarySave(p.day, p.text); setSaved("saved"); } catch (e) { notify.error(errorMessage(e)); }
  }, []);

  useEffect(() => {
    let live = true;
    setLoaded(false); setMade({});
    void flush().then(() => ipc.diaryGet(day)).then((t) => { if (live) { setText(t); setLoaded(true); setSaved("idle"); } }).catch((e) => notify.error(errorMessage(e)));
    return () => { live = false; };
  }, [day, flush]);
  useEffect(() => () => { void flush(); }, [flush]);

  const edit = (t: string) => {
    setText(t);
    pending.current = { day, text: t };
    setSaved("idle");
    clearTimeout(timer.current);
    timer.current = setTimeout(() => void flush(), 400);
  };
  const shift = (n: number) => { const d = parseYmd(day); d.setDate(d.getDate() + n); setDay(ymd(d)); };
  const today = ymd(new Date());

  const makeTask = async (key: string, title: string, due: string | null) => {
    try {
      const t = await ipc.taskQuickAdd(title);
      if (due) await ipc.taskUpdate(t.id, { title: t.title, description: t.description, project: t.project, priority: t.priority, planned_date: t.planned_date, due_date: due, note_id: t.note_id });
      setMade((m) => ({ ...m, [key]: t.title }));
      notify.success(`Task added: “${t.title}”`);
    } catch (e) { notify.error(errorMessage(e)); }
  };
  const fromSelection = () => {
    const el = area.current;
    const sel = el ? text.slice(el.selectionStart, el.selectionEnd).trim() : "";
    if (!sel) return notify.info("Select some text first");
    void makeTask(`sel:${sel}`, titleOf(sel) || sel.slice(0, 300), detectTodos(sel, new Date())[0]?.due ?? null);
  };
  const todos = pickup.value ? detectTodos(text, new Date()) : [];

  return (
    <div className="mx-auto grid max-w-5xl gap-6 lg:grid-cols-[1fr_14rem]">
      <section aria-label="Diary entry" className="min-w-0">
        <div className="mb-3 flex items-center gap-1">
          <Button variant="ghost" size="icon" aria-label="Previous day" onClick={() => shift(-1)}><ChevronLeft /></Button>
          <h2 className="min-w-0 flex-1 text-center text-lg font-semibold tracking-tight">{dayLabel(parseYmd(day))}</h2>
          <Button variant="ghost" size="icon" aria-label="Next day" disabled={day >= today} onClick={() => shift(1)}><ChevronRight /></Button>
        </div>
        <textarea
          ref={area} value={text} disabled={!loaded} onChange={(e) => edit(e.target.value)} aria-label="Diary text" placeholder="How was your day?" maxLength={200_000}
          className="min-h-[22rem] w-full resize-y rounded-xl border bg-card px-6 py-5 text-[15px] leading-[1.7] outline-none placeholder:text-muted-foreground focus-visible:ring-2 focus-visible:ring-ring"
        />
        <div className="mt-2 flex flex-wrap items-center gap-3 text-xs text-muted-foreground">
          <span aria-live="polite">{saved === "saving" ? "Saving…" : saved === "saved" ? "Saved" : ""}</span>
          <span>{text.trim() ? text.trim().split(/\s+/).length : 0} words</span>
          <Button variant="ghost" size="sm" className="ml-auto" onClick={fromSelection}><ListPlus />Make task from selection</Button>
        </div>
        {todos.length > 0 && (
          <ul aria-label="Things that look like to-dos" className="mt-3 space-y-1 rounded-xl border bg-card p-3">
            {todos.map((t) => (
              <li key={t.line} className="flex items-center gap-2 text-[13px]">
                <span className="min-w-0 flex-1 truncate underline decoration-primary/60 decoration-dotted underline-offset-4">{t.title}{t.due ? ` · ${t.due}` : ""}</span>
                {made[`line:${t.text}`] ? <span className="inline-flex items-center gap-1 rounded-md bg-primary/10 px-1.5 py-0.5 text-[11px] text-primary"><Check aria-hidden className="size-3" />Task made</span>
                  : <Button size="sm" variant="outline" onClick={() => void makeTask(`line:${t.text}`, t.title, t.due)}>Make task</Button>}
              </li>
            ))}
          </ul>
        )}
      </section>
      <aside className="space-y-4">
        <Button variant="outline" size="sm" className="w-full" onClick={() => void flush().then(onLock)}><Lock />Lock now</Button>
        <label className="flex items-start gap-2 text-xs text-muted-foreground">
          <Switch checked={pickup.value === true} onCheckedChange={(v) => void pickup.set(v)} aria-label="Spot things to do" />
          <span><span className="block font-medium text-foreground">Spot things to do</span>Underlines lines like “I need to submit… by Friday”. Off by default; it all happens on this computer.</span>
        </label>
        <div>
          <label htmlFor="diary-search" className="mb-1 block text-xs font-medium text-muted-foreground">Search the diary</label>
          <Input id="diary-search" type="search" value={query} maxLength={100} onChange={(e) => setQuery(e.target.value)} onKeyDown={(e) => e.key === "Escape" && setQuery("")} placeholder="Words from an entry" className="h-8" />
          {query.trim() && (
            <ul aria-label="Search results" className="mt-2 max-h-60 space-y-1 overflow-auto">
              {(results.data ?? []).map((h) => (
                <li key={h.day}><button type="button" onClick={() => setDay(h.day)} className="w-full rounded px-2 py-1 text-left text-xs outline-none hover:bg-accent focus-visible:ring-2 focus-visible:ring-ring"><span className="block font-medium tabular-nums">{h.day}</span><span className="block text-muted-foreground">{h.snippet}</span></button></li>
              ))}
              {results.data && results.data.length === 0 && <li className="px-2 text-xs text-muted-foreground">No entry has those words.</li>}
            </ul>
          )}
        </div>
        <div>
          <h3 className="mb-1 text-xs font-medium text-muted-foreground">Earlier entries</h3>
          <ul className="max-h-72 space-y-0.5 overflow-auto">
            {(days.data ?? []).slice(0, 60).map((d) => (
              <li key={d}><button type="button" aria-current={d === day ? "date" : undefined} onClick={() => setDay(d)} className={`w-full rounded px-2 py-1 text-left text-xs outline-none hover:bg-accent focus-visible:ring-2 focus-visible:ring-ring ${d === day ? "bg-accent font-medium" : ""}`}>{d}</button></li>
            ))}
            {(days.data ?? []).length === 0 && <li className="px-2 text-xs text-muted-foreground">Nothing yet.</li>}
          </ul>
        </div>
      </aside>
    </div>
  );
}

/** Diary: a private, password-locked journal. The text is encrypted on this computer and never leaves it. */
export function DiaryPage() {
  const qc = useQueryClient();
  const status = useQuery({ queryKey: STATUS, queryFn: () => ipc.diaryStatus() });
  const lock = useCallback(async () => { await ipc.diaryLock(); await qc.invalidateQueries({ queryKey: STATUS }); }, [qc]);
  const unlocked = status.data?.unlocked === true;
  // The shell locks the diary when the dashboard is put away; drop what is on screen to match.
  useEffect(() => onCommand((c) => { if (c === "diary-locked") void qc.invalidateQueries({ queryKey: STATUS }); }), [qc]);
  const [newKey, setNewKey] = useState<string | null>(null);

  // Locks after five idle minutes, and when you leave the page.
  useEffect(() => {
    if (!unlocked) return;
    let t = setTimeout(() => void lock(), IDLE_LOCK_MS);
    const reset = () => { clearTimeout(t); t = setTimeout(() => void lock(), IDLE_LOCK_MS); };
    const events = ["keydown", "pointerdown", "pointermove"] as const;
    for (const e of events) window.addEventListener(e, reset);
    return () => { clearTimeout(t); for (const e of events) window.removeEventListener(e, reset); void ipc.diaryLock(); void qc.invalidateQueries({ queryKey: STATUS }); };
  }, [unlocked, lock, qc]);

  return (
    <div>
      <PageHeader title="Diary" description="Private. Locked with your password." actions={<BookHeart aria-hidden className="size-5 text-primary" />} />
      {status.data && (newKey ? <RecoveryKeyCard recoveryKey={newKey} onDone={() => setNewKey(null)} /> : unlocked ? <Editor onLock={() => void lock()} /> : <PasswordForm configured={status.data.configured} onDone={(k) => k && setNewKey(k)} />)}
    </div>
  );
}
