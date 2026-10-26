import { useCallback, useEffect, useState } from "react";
import { api, DelResult, Drive, fmt, Pick } from "./api";
import Explorer from "./Explorer";
import { Assistant, Clean, Dups, Report } from "./Views";
import Settings from "./Settings";
import Residus from "./Residus";
import OneDrive from "./OneDrive";
import Sante from "./Sante";

type Tab = "explorer" | "clean" | "dups" | "ai" | "report" | "residus" | "onedrive" | "sante" | "settings";
const TABS: [Tab, string, string][] = [
  ["explorer", "Explorateur", "◎"], ["clean", "Nettoyage rapide", "✦"], ["dups", "Doublons", "⧉"],
  ["ai", "Assistant IA", "✨"], ["report", "Rapport", "▤"], ["residus", "Résidus logiciels", "🧹"], ["onedrive", "OneDrive", "☁"], ["sante", "Santé du PC", "♥"], ["settings", "Réglages", "⚙"],
];

export default function App() {
  const [tab, setTab] = useState<Tab>("explorer");
  const [drives, setDrives] = useState<Drive[]>([]);
  const [selected, setSelected] = useState<Map<string, number>>(new Map());
  const [toast, setToast] = useState("");
  const [confirm, setConfirm] = useState(false);
  const [version, setVersion] = useState(0);

  const [theme, setTheme] = useState<"dark" | "light">(() => (localStorage.getItem("theme") === "light" ? "light" : "dark"));
  useEffect(() => { document.documentElement.dataset.theme = theme; localStorage.setItem("theme", theme); }, [theme]);

  const refreshDrives = useCallback(() => api.drives().then(setDrives), []);
  useEffect(() => { refreshDrives(); }, [refreshDrives]);
  const notify = useCallback((m: string) => { setToast(m); window.setTimeout(() => setToast((t) => (t === m ? "" : t)), 6000); }, []);

  const toggle = (p: string, s: number) =>
    setSelected((m) => { const n = new Map(m); n.has(p) ? n.delete(p) : n.set(p, s); return n; });
  const setMany = (items: { path: string; size: number }[], on: boolean) =>
    setSelected((m) => { const n = new Map(m); items.forEach((i) => (on ? n.set(i.path, i.size) : n.delete(i.path))); return n; });
  const addPicks = (p: Pick[]) => setMany(p, true);

  const total = [...selected.values()].reduce((a, b) => a + b, 0);
  const scanned = drives.filter((d) => d.scanned_at);

  return (
    <div className="app">
      <nav className="nav">
        <div className="brand"><span className="logo" />DiskLens</div>
        {TABS.map(([id, label, ico]) => (
          <button key={id} className={tab === id ? "on" : ""} onClick={() => setTab(id)}><span>{ico}</span>{label}</button>
        ))}
        <div className="grow" />
        <button className="theme" onClick={() => setTheme(theme === "dark" ? "light" : "dark")}><span>{theme === "dark" ? "☀" : "☾"}</span>{theme === "dark" ? "Mode clair" : "Mode sombre"}</button>
        <div className="drives">
          {drives.map((d) => {
            const used = d.total - d.free, pct = (used / d.total) * 100;
            return (
              <div key={d.key} className="drive">
                <div className="between"><b>{d.key}</b><span className="muted small">{fmt(d.free)} libres</span></div>
                <div className="meter"><i className={pct > 90 ? "hot" : pct > 75 ? "warm" : ""} style={{ width: pct + "%" }} /></div>
              </div>
            );
          })}
        </div>
      </nav>

      <main>
        <div hidden={tab !== "explorer"} className="pane">
          <Explorer drives={drives} refreshDrives={refreshDrives} selected={selected} toggle={toggle} notify={notify} version={version} />
        </div>
        {tab === "clean" && <div className="pane"><Clean selected={selected} toggle={toggle} setMany={setMany} drivesReady={scanned.length > 0} /></div>}
        {tab === "dups" && <div className="pane"><Dups selected={selected} toggle={toggle} setMany={setMany} drivesReady={scanned.length > 0} /></div>}
        {tab === "ai" && <div className="pane"><Assistant addPicks={addPicks} notify={notify} /></div>}
        {tab === "report" && <div className="pane"><Report drives={drives.map((d) => d.key)} notify={notify} /></div>}
        {tab === "residus" && <div className="pane"><Residus notify={notify} /></div>}
        {tab === "onedrive" && <div className="pane"><OneDrive notify={notify} /></div>}
        {tab === "sante" && <div className="pane"><Sante notify={notify} /></div>}
        {tab === "settings" && <div className="pane"><Settings notify={notify} /></div>}
      </main>

      {selected.size > 0 && (
        <div className="basket">
          <div><b>{selected.size}</b> élément{selected.size > 1 ? "s" : ""} · <b>{fmt(total)}</b> à libérer</div>
          <div className="row">
            <button className="btn" onClick={() => setSelected(new Map())}>Vider</button>
            <button className="btn danger" onClick={() => setConfirm(true)}>Supprimer…</button>
          </div>
        </div>
      )}
      {toast && <div className="toast" onClick={() => setToast("")}>{toast}</div>}
      {confirm && (
        <DeleteModal items={[...selected.entries()].map(([path, size]) => ({ path, size }))}
          onClose={() => setConfirm(false)}
          onDone={(res, trash) => {
            const ok = new Set(res.filter((r) => r.ok).map((r) => r.path));
            setSelected((m) => { const n = new Map(m); ok.forEach((p) => n.delete(p)); return n; });
            setVersion((v) => v + 1); refreshDrives();
            notify(trash
              ? `${ok.size} élément(s) envoyé(s) à la corbeille · espace libéré après vidage · ${res.filter((r) => !r.ok).length} échec(s)`
              : `${fmt(res.reduce((s, r) => s + r.freed, 0))} libérés · ${res.filter((r) => !r.ok).length} échec(s)`);
          }} />
      )}
    </div>
  );
}

function DeleteModal({ items, onClose, onDone }: { items: { path: string; size: number }[]; onClose: () => void; onDone: (r: DelResult[], trash: boolean) => void }) {
  const [prot, setProt] = useState<(string | null)[] | null>(null);
  const [error, setError] = useState("");
  const [checkVersion, setCheckVersion] = useState(0);
  const [busy, setBusy] = useState(false);
  const [trash, setTrash] = useState(() => { try { return localStorage.getItem("trash") !== "0"; } catch { return true; } });
  const setTrash_ = (v: boolean) => { setTrash(v); try { localStorage.setItem("trash", v ? "1" : "0"); } catch { /* ignoré */ } };
  const [res, setRes] = useState<DelResult[] | null>(null);
  useEffect(() => {
    let active = true;
    setProt(null); setError("");
    api.protection(items.map((i) => i.path)).then((reasons) => {
      if (!active) return;
      if (reasons.length !== items.length) throw new Error("Vérification incomplète des protections");
      setProt(reasons);
    }).catch((e) => { if (active) setError(String(e)); });
    return () => { active = false; };
  }, [checkVersion]); // eslint-disable-line
  const ok = prot ? items.filter((_, i) => !prot[i]) : [];
  const total = ok.reduce((s, i) => s + i.size, 0);
  const go = async () => {
    if (!prot || error || !ok.length) return;
    setBusy(true); setError("");
    try {
      const r = await api.del(ok, trash);
      setRes(r);
      onDone(r, trash);
    } catch (e) { setError(String(e)); }
    finally { setBusy(false); }
  };

  return (
    <div className="overlay" onClick={() => !busy && onClose()}>
      <div className="modal" onClick={(e) => e.stopPropagation()}>
        <h2>{res ? "Terminé" : trash ? "Envoi à la corbeille" : "Suppression définitive"}</h2>
        {!res && (
          <>
            <label className="row"><input type="checkbox" checked={trash} onChange={(e) => setTrash_(e.target.checked)} /> Envoyer à la corbeille (annulable ; l'espace n'est libéré qu'au vidage de la corbeille)</label>
            {!trash && <p className="warn">⚠ Ces éléments seront supprimés <b>définitivement</b> (pas de corbeille). Un journal est tenu dans le dossier de données de l'application.</p>}
          </>
        )}
        {!res && !prot && !error && <p className="muted">Vérification des protections…</p>}
        {error && <p className="warn">{error} {!res && <button className="btn" disabled={busy} onClick={() => setCheckVersion((v) => v + 1)}>Réessayer la vérification</button>}</p>}
        <div className="list-box">
          {(res ?? items.map((it, i) => ({ path: it.path, ok: !!prot && !prot[i], freed: it.size, error: prot?.[i] ? `Protégé: ${prot[i]}` : null }))).map((r, i) => (
            <div key={i} className={"rowitem" + (r.error ? " bad" : "")}>
              <div className="grow"><div className="ellipsis" title={r.path}>{r.path}</div>{r.error && <div className="small">{r.error}</div>}</div>
              {res ? <span>{r.ok ? "✅" : "❌"}</span> : <b>{fmt(items[i].size)}</b>}
            </div>
          ))}
        </div>
        <div className="row between">
          {res ? <b>{trash ? `${res.filter((r) => r.ok).length} élément(s) dans la corbeille · espace libéré après vidage` : `${fmt(res.reduce((s, r) => s + r.freed, 0))} libérés`}</b> : <b>{prot ? `${ok.length} élément(s) · ${fmt(total)}` : "Vérification des protections…"}</b>}
          <div className="row">
            <button className="btn" disabled={busy} onClick={onClose}>{res ? "Fermer" : "Annuler"}</button>
            {!res && <button className="btn danger" disabled={busy || !prot || !!error || !ok.length} onClick={go}>{busy ? "Traitement…" : trash ? `Mettre à la corbeille · ${fmt(total)}` : `Supprimer ${fmt(total)}`}</button>}
          </div>
        </div>
      </div>
    </div>
  );
}
