import { useCallback, useEffect, useRef, useState } from "react";
import { api, ageLabel, Drive, Explain, fmt, Item, Listing, ScanProgress } from "./api";
import Treemap from "./Treemap";

type Props = {
  drives: Drive[]; refreshDrives: () => void;
  selected: Map<string, number>; toggle: (path: string, size: number) => void;
  notify: (m: string) => void; version: number;
};

const RISK: Record<string, string> = { sans_risque: "Sans risque", faible: "Risque faible", moyen: "À vérifier", eleve: "Risque élevé" };

export default function Explorer({ drives, refreshDrives, selected, toggle, notify, version }: Props) {
  const [drive, setDrive] = useState<string>("");
  const [path, setPath] = useState<string>("");
  const [listing, setListing] = useState<Listing | null>(null);
  const [prog, setProg] = useState<ScanProgress | null>(null);
  const [scanning, setScanning] = useState(false);
  const [hover, setHover] = useState<Item | null>(null);
  const [view, setView] = useState<"map" | "list" | "top">("map");
  const [topKind, setTopKind] = useState<"file" | "dir">("file");
  const [topExt, setTopExt] = useState("");
  const [topAge, setTopAge] = useState(0);
  const [top, setTop] = useState<Item[]>([]);
  const [focus, setFocus] = useState<Item | null>(null);
  const [exp, setExp] = useState<Explain | null>(null);
  const [expBusy, setExpBusy] = useState(false);
  const [open_, setOpen_] = useState<Set<string>>(new Set());
  const [kids, setKids] = useState<Record<string, Item[]>>({});
  const box = useRef<HTMLDivElement>(null);
  const [size, setSize] = useState({ w: 800, h: 480 });

  useEffect(() => { if (!drive && drives.length) setDrive(drives[0].key); }, [drives, drive]);
  useEffect(() => {
    const un = api.onProgress(setProg);
    return () => { un.then((f) => f()); };
  }, []);
  useEffect(() => {
    if (!box.current) return;
    const ro = new ResizeObserver(() => box.current && setSize({ w: box.current.clientWidth, h: box.current.clientHeight }));
    ro.observe(box.current);
    return () => ro.disconnect();
  }, [listing]);

  const load = useCallback(async (p: string) => {
    try {
      const l = await api.children(p);
      setListing(l); setPath(l.path); setFocus(null); setHover(null); setExp(null); setOpen_(new Set()); setKids({});
    } catch (e) { notify(String(e)); }
  }, [notify]);

  useEffect(() => {
    const d = drives.find((x) => x.key === drive);
    if (d?.scanned_at && !listing) load(d.key + "\\");
  }, [drive, drives]); // eslint-disable-line

  useEffect(() => {
    if (!version || !path) return;
    api.children(path).then((l) => { setListing(l); setFocus(null); setHover(null); setExp(null); }).catch(() => load(drive + "\\"));
  }, [version]); // eslint-disable-line

  useEffect(() => {
    if (view !== "top" || !path) return;
    api.top(path, topKind, 20, topExt, topAge || undefined).then(setTop).catch((e) => notify(String(e)));
  }, [view, path, topKind, topExt, topAge, version]); // eslint-disable-line

  const scan = async () => {
    setScanning(true); setProg(null); setListing(null);
    try {
      const r = await api.scan(drive + "\\");
      if (r.cancelled) notify("Scan annulé");
      else {
        notify(`${drive} analysé: ${fmt(r.size)} dans ${r.files.toLocaleString("fr")} fichiers (${r.secs.toFixed(1)} s)`);
        await load(drive + "\\");
      }
      refreshDrives();
    } catch (e) { notify(String(e)); }
    setScanning(false);
  };

  const crumbs = path ? path.replace(/\\$/, "").split("\\") : [];
  const goCrumb = (i: number) => load(crumbs.slice(0, i + 1).join("\\") + "\\");
  const open = (it: Item) => it.kind === "dir" && load(it.path);
  const explain = async (it: Item) => {
    setExpBusy(true); setExp(null);
    try { setExp(await api.explain(it.path)); } catch (e) { notify(String(e)); }
    setExpBusy(false);
  };
  const toggleOpen = async (it: Item) => {
    const k = it.path;
    if (open_.has(k)) { setOpen_((s) => { const n = new Set(s); n.delete(k); return n; }); return; }
    if (!kids[k]) {
      try { const l = await api.children(k, 200); setKids((m) => ({ ...m, [k]: l.items })); } catch (e) { notify(String(e)); return; }
    }
    setOpen_((s) => new Set(s).add(k));
  };
  const rows: { it: Item; depth: number }[] = [];
  const walk = (list: Item[], depth: number) => list.forEach((it) => {
    rows.push({ it, depth });
    if (it.kind === "dir" && open_.has(it.path) && kids[it.path]) walk(kids[it.path], depth + 1);
  });
  walk(listing?.items ?? [], 0);
  const cur = drives.find((d) => d.key === drive);
  const items = listing?.items ?? [];

  return (
    <div className="explorer">
      <div className="toolbar">
        <select value={drive} onChange={(e) => { setDrive(e.target.value); setListing(null); setPath(""); }}>
          {drives.map((d) => <option key={d.key} value={d.key}>{d.key} {d.label && `· ${d.label}`} ({fmt(d.free)} libres)</option>)}
        </select>
        {scanning
          ? <button className="btn danger" onClick={() => api.cancelScan()}>Annuler</button>
          : <button className="btn primary" onClick={scan}>{cur?.scanned_at ? "Réanalyser" : "Analyser"}</button>}
        <div className="seg">
          <button className={view === "map" ? "on" : ""} onClick={() => setView("map")}>Carte</button>
          <button className={view === "list" ? "on" : ""} onClick={() => setView("list")}>Liste</button>
          <button className={view === "top" ? "on" : ""} onClick={() => setView("top")}>Top 20</button>
        </div>
        {listing && <span className="muted">{fmt(listing.size)} · {listing.files.toLocaleString("fr")} fichiers</span>}
      </div>

      {scanning && (
        <div className="scanbox">
          <div className="pulse" />
          <div>
            <b>{prog ? prog.files.toLocaleString("fr") : 0}</b> fichiers · <b>{prog ? fmt(prog.bytes) : "0 o"}</b> · {prog?.errors ?? 0} dossiers inaccessibles
            <div className="muted ellipsis">{prog?.current}</div>
          </div>
        </div>
      )}

      {!scanning && !listing && (
        <div className="empty">
          <h2>Voir ce qui occupe {drive || "le disque"}</h2>
          <p>Lance l'analyse : la carte montre les dossiers proportionnellement à l'espace réellement alloué. Clique pour descendre, clic droit ou case pour sélectionner.</p>
        </div>
      )}

      {listing && !scanning && (
        <>
          <div className="crumbs">
            {crumbs.map((c, i) => (
              <span key={i}><button onClick={() => goCrumb(i)}>{c}</button>{i < crumbs.length - 1 && " › "}</span>
            ))}
          </div>
          <div className="split">
            <div className="main" ref={box}>
              {view === "top" ? (
                <>
                  <div className="toolbar">
                    <div className="seg">
                      <button className={topKind === "file" ? "on" : ""} onClick={() => setTopKind("file")}>Fichiers</button>
                      <button className={topKind === "dir" ? "on" : ""} onClick={() => setTopKind("dir")}>Dossiers</button>
                    </div>
                    {topKind === "file" && <input placeholder="extensions: mp4,zip" value={topExt} onChange={(e) => setTopExt(e.target.value)} />}
                    <select value={topAge} onChange={(e) => setTopAge(Number(e.target.value))}>
                      <option value={0}>Tout âge</option><option value={90}>&gt; 3 mois</option><option value={365}>&gt; 1 an</option>
                    </select>
                  </div>
                  <table className="list">
                    <thead><tr><th></th><th>Nom</th><th className="r">Taille</th><th className="r">Modifié</th></tr></thead>
                    <tbody>
                      {top.map((it) => (
                        <tr key={it.path} className={focus === it ? "focus" : ""} onClick={() => setFocus(it)} onDoubleClick={() => api.reveal(it.path)}>
                          <td><input type="checkbox" checked={selected.has(it.path)} onChange={() => toggle(it.path, it.size)} onClick={(e) => e.stopPropagation()} /></td>
                          <td title={it.path}>{it.kind === "dir" ? "📁" : "📄"} {it.name}<div className="muted ellipsis">{it.path}</div></td>
                          <td className="r">{fmt(it.size)}</td>
                          <td className="r">{ageLabel(it.age_days)}</td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                </>
              ) : view === "map" ? (
                <Treemap items={items} width={size.w} height={size.h}
                  selected={new Set(selected.keys())}
                  onOpen={open}
                  onHover={(i) => { if (i && i.path !== hover?.path) { setHover(i); setExp(null); } }}
                  onToggle={(i) => i.kind !== "small" ? toggle(i.path, i.size) : notify("Ces petits fichiers sont regroupés: ouvre le dossier parent pour les détails.")} />
              ) : (
                <table className="list">
                  <thead><tr><th></th><th>Nom</th><th className="r">Taille</th><th className="r">Fichiers</th><th className="r">Modifié</th></tr></thead>
                  <tbody>
                    {rows.map(({ it, depth }) => (
                      <tr key={it.path + "|" + it.name + "|" + depth} className={focus === it ? "focus" : ""} onClick={() => setFocus(it)} onDoubleClick={() => open(it)}>
                        <td>{it.kind !== "small" && <input type="checkbox" checked={selected.has(it.path)} onChange={() => toggle(it.path, it.size)} onClick={(e) => e.stopPropagation()} />}</td>
                        <td style={{ paddingLeft: 8 + depth * 18 }}>
                          {it.kind === "dir" && it.files > 0
                            ? <button className="twist" onClick={(e) => { e.stopPropagation(); toggleOpen(it); }}>{open_.has(it.path) ? "▾" : "▸"}</button>
                            : <span className="twist" />}
                          {it.kind === "dir" ? "📁" : it.kind === "file" ? "📄" : "▫️"} {it.name}
                        </td>
                        <td className="r">
                          <div className="bar" style={{ width: `${Math.max(2, (it.size / (listing.size || 1)) * 100)}%` }} />
                          {fmt(it.size)}
                        </td>
                        <td className="r">{it.files.toLocaleString("fr")}</td>
                        <td className="r">{ageLabel(it.age_days)}</td>
                      </tr>
                    ))}
                    {listing.hidden_count > 0 && <tr><td></td><td className="muted">… {listing.hidden_count} éléments plus petits</td><td className="r">{fmt(listing.hidden_size)}</td><td></td><td></td></tr>}
                  </tbody>
                </table>
              )}
            </div>
            <aside className="side">
              {(() => {
                const it = view === "map" ? hover ?? focus : focus;
                if (!it) return <p className="muted">Survole un élément pour le détail (il reste affiché quand tu quittes la carte). Ctrl+clic ou clic droit : sélectionner.</p>;
                return (
                  <>
                    <h3 className="ellipsis" title={it.name}>{it.name}</h3>
                    <div className="big">{fmt(it.size)}</div>
                    <div className="muted">{it.files.toLocaleString("fr")} fichiers · modifié {ageLabel(it.age_days)}</div>
                    <div className="muted ellipsis" title={it.path}>{it.path}</div>
                    {it.kind !== "small" && (
                      <div className="row">
                        <button className="btn" onClick={() => toggle(it.path, it.size)}>{selected.has(it.path) ? "Retirer" : "Sélectionner"}</button>
                        <button className="btn" onClick={() => api.reveal(it.path)}>Ouvrir</button>
                        <button className="btn ai" onClick={() => explain(it)}>✨ Expliquer</button>
                      </div>
                    )}
                  </>
                );
              })()}
              {expBusy && <div className="ai-card"><div className="pulse" /> Analyse en cours…</div>}
              {exp && (
                <div className={"ai-card risk-" + exp.risque}>
                  <div className="tag">{RISK[exp.risque] ?? exp.risque}{exp.se_regenere ? " · se régénère" : ""}</div>
                  <p>{exp.resume}</p>
                  <p><b>{exp.supprimable ? "Supprimable" : "À ne pas supprimer à la légère"}.</b> {exp.conseil}</p>
                </div>
              )}
            </aside>
          </div>
        </>
      )}
    </div>
  );
}
