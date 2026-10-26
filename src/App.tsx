import { useCallback, useEffect, useState } from "react";
import { api, DelResult, Drive, fmt, Pick, ResidusReport } from "./api";
import Explorer from "./Explorer";
import { Assistant, Clean, Dups, Report } from "./Views";
import Settings from "./Settings";
import Residus from "./Residus";
import OneDrive from "./OneDrive";
import Sante from "./Sante";
import Accueil from "./Accueil";
import Logiciels from "./Logiciels";
import Icon from "./Icon";
import { lireRoute, Route, routes } from "./navigation";

const groupes = ["Stockage", "Applications", "Suivi", "Outils"];

export default function App() {
  const [tab, setTab] = useState<Route>(() => lireRoute(typeof window === "undefined" ? "" : window.location.hash));
  const [visites, setVisites] = useState<Set<Route>>(() => new Set([tab]));
  const [drives, setDrives] = useState<Drive[]>([]);
  const [selected, setSelected] = useState<Map<string, number>>(new Map());
  const [toast, setToast] = useState("");
  const [confirm, setConfirm] = useState(false);
  const [version, setVersion] = useState(0);
  const [driveError, setDriveError] = useState("");
  const [drivesLoading, setDrivesLoading] = useState(true);
  const [rapportResidus, setRapportResidus] = useState<ResidusReport | null>(null);

  const [theme, setTheme] = useState<"dark" | "light">(() => { try { return localStorage.getItem("theme") === "dark" ? "dark" : "light"; } catch { return "light"; } });
  useEffect(() => { document.documentElement.dataset.theme = theme; try { localStorage.setItem("theme", theme); } catch { /* Préférence en mémoire. */ } }, [theme]);
  useEffect(() => {
    const changer = () => setTab(lireRoute(window.location.hash));
    window.addEventListener("hashchange", changer);
    return () => window.removeEventListener("hashchange", changer);
  }, []);
  useEffect(() => { setVisites((v) => v.has(tab) ? v : new Set([...v, tab])); }, [tab]);

  const refreshDrives = useCallback(async () => {
    setDrivesLoading(true); setDriveError("");
    try { setDrives(await api.drives()); }
    catch (e) { setDriveError(`Lecture des disques impossible : ${String(e)}`); }
    finally { setDrivesLoading(false); }
  }, []);
  useEffect(() => { refreshDrives(); }, [refreshDrives]);
  const notify = useCallback((m: string) => { setToast(m); window.setTimeout(() => setToast((t) => (t === m ? "" : t)), 6000); }, []);

  const toggle = (p: string, s: number) =>
    setSelected((m) => { const n = new Map(m); n.has(p) ? n.delete(p) : n.set(p, s); return n; });
  const setMany = (items: { path: string; size: number }[], on: boolean) =>
    setSelected((m) => { const n = new Map(m); items.forEach((i) => (on ? n.set(i.path, i.size) : n.delete(i.path))); return n; });
  const addPicks = (p: Pick[]) => setMany(p, true);

  const total = [...selected.values()].reduce((a, b) => a + b, 0);
  const scanned = drives.filter((d) => d.scanned_at);
  const actif = routes[tab];
  const affiche = (id: Route) => tab === id || visites.has(id);

  return (
    <div className="app">
      <a className="skip-link" href="#contenu" onClick={(e) => { e.preventDefault(); document.getElementById("contenu")?.focus(); }}>Aller au contenu</a>
      <nav className="nav" aria-label="Navigation principale">
        <a href="#home" className="brand" aria-label="DiskLens, accueil"><span className="brand-mark"><Icon name="disk" size={24} /></span><span>DiskLens<small>Stockage et applications</small></span></a>
        <a href="#home" className={`nav-link${tab === "home" ? " on" : ""}`} aria-label="Vue d’ensemble" title="Vue d’ensemble" aria-current={tab === "home" ? "page" : undefined}><Icon name="home" /><span>Vue d’ensemble</span></a>
        <div className="nav-sections">{groupes.map((groupe) => <div className="nav-group" key={groupe}><div className="nav-group-label">{groupe}</div>{(Object.keys(routes) as Route[]).filter((id) => routes[id].group === groupe).map((id) => <a key={id} href={`#${id}`} className={`nav-link${tab === id ? " on" : ""}`} aria-label={routes[id].label} title={routes[id].label} aria-current={tab === id ? "page" : undefined}><Icon name={routes[id].icon} /><span>{routes[id].label}</span></a>)}</div>)}</div>
        <div className="nav-footer"><button className="nav-link theme" aria-label={theme === "dark" ? "Passer en mode clair" : "Passer en mode sombre"} onClick={() => setTheme(theme === "dark" ? "light" : "dark")}><Icon name={theme === "dark" ? "sun" : "moon"} /><span>{theme === "dark" ? "Passer en mode clair" : "Passer en mode sombre"}</span></button><span className="app-local">Application Windows locale</span></div>
      </nav>

      <main id="contenu" tabIndex={-1}>
        <header className="workspace-bar"><div><span className="workspace-group">{actif.group}</span><h1>{actif.label}</h1></div><div className="workspace-context"><Icon name={actif.icon} size={18} /><span>DiskLens</span></div></header>
        {tab === "home" && <div className="pane"><Accueil drives={drives} loading={drivesLoading} error={driveError} refresh={refreshDrives} /></div>}
        {affiche("explorer") && <div hidden={tab !== "explorer"} className="pane">
          <Explorer drives={drives} refreshDrives={refreshDrives} selected={selected} toggle={toggle} notify={notify} version={version} />
        </div>}
        {affiche("clean") && <div hidden={tab !== "clean"} className="pane"><Clean selected={selected} toggle={toggle} setMany={setMany} drivesReady={scanned.length > 0} /></div>}
        {affiche("dups") && <div hidden={tab !== "dups"} className="pane"><Dups selected={selected} toggle={toggle} setMany={setMany} drivesReady={scanned.length > 0} /></div>}
        {affiche("ai") && <div hidden={tab !== "ai"} className="pane"><Assistant addPicks={addPicks} notify={notify} /></div>}
        {affiche("report") && <div hidden={tab !== "report"} className="pane"><Report drives={drives.map((d) => d.key)} notify={notify} /></div>}
        {affiche("logiciels") && <div hidden={tab !== "logiciels"} className="pane"><Logiciels actif={tab === "logiciels"} notify={notify} onResidus={(r) => { setRapportResidus(r); window.location.hash = "residus"; }} /></div>}
        {affiche("residus") && <div hidden={tab !== "residus"} className="pane"><Residus notify={notify} rapport={rapportResidus} onRapport={setRapportResidus} /></div>}
        {affiche("onedrive") && <div hidden={tab !== "onedrive"} className="pane"><OneDrive notify={notify} /></div>}
        {affiche("sante") && <div hidden={tab !== "sante"} className="pane"><Sante notify={notify} /></div>}
        {affiche("settings") && <div hidden={tab !== "settings"} className="pane"><Settings notify={notify} /></div>}
      </main>

      {selected.size > 0 && (
        <div className="basket">
          <div><b>{selected.size}</b> élément{selected.size > 1 ? "s" : ""} sélectionné{selected.size > 1 ? "s" : ""} · <b>{fmt(total)}</b> à examiner</div>
          <div className="row">
            <button className="btn" onClick={() => setSelected(new Map())}>Vider</button>
            <button className="btn danger" onClick={() => setConfirm(true)}>Supprimer…</button>
          </div>
        </div>
      )}
      {toast && <div className="toast" role="status" aria-live="polite"><span>{toast}</span><button className="icon-button" aria-label="Fermer la notification" onClick={() => setToast("")}><Icon name="close" size={18} /></button></div>}
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
