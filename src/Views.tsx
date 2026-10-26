import { useEffect, useRef, useState } from "react";
import { api, ageLabel, Candidate, DupGroup, fmt, Msg, Pick, ReportData } from "./api";
import Mensuel from "./Mensuel";

type Sel = { selected: Map<string, number>; toggle: (p: string, s: number) => void; setMany: (items: { path: string; size: number }[], on: boolean) => void };
const RISK_LABEL: Record<string, string> = { safe: "Sans risque", low: "Réinstallable", medium: "À vérifier" };

// ---------------- Nettoyage rapide (déterministe) ----------------
export function Clean({ selected, toggle, setMany, drivesReady }: Sel & { drivesReady: boolean }) {
  const [list, setList] = useState<Candidate[] | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const run = async () => {
    setBusy(true); setError("");
    try { setList(await api.candidates()); }
    catch (e) { setError(String(e)); }
    finally { setBusy(false); }
  };
  useEffect(() => { if (drivesReady) run(); }, [drivesReady]);

  if (!drivesReady) return <div className="empty"><h2>Analyse d'abord un disque</h2><p>Le nettoyage rapide s'appuie sur les résultats du scan (onglet Explorateur).</p></div>;
  if (!list) return <div className="empty">{busy ? <><div className="pulse" /> Recherche…</> : <><p>Recherche impossible : {error || "aucun résultat"}</p><button className="btn" onClick={run}>Réessayer</button></>}</div>;
  const cats = [...new Set(list.map((c) => c.category))];
  const safe = list.filter((c) => c.risk === "safe");
  const safeTotal = safe.reduce((s, c) => s + c.size, 0);

  return (
    <div className="page">
      <div className="hero">
        <div>
          <div className="big">{fmt(list.reduce((s, c) => s + c.size, 0))}</div>
          <div className="muted">récupérables détectés · dont {fmt(safeTotal)} sans aucun risque</div>
        </div>
        <div className="row">
          <button className="btn primary" onClick={() => setMany(safe, true)}>Sélectionner le sans-risque</button>
          <button className="btn" onClick={() => { const c = list.filter((x) => /cache/i.test(x.category) || /cache/i.test(x.path)); setMany(c, true); }}>Sélectionner les caches ({fmt(list.filter((x) => /cache/i.test(x.category) || /cache/i.test(x.path)).reduce((s, x) => s + x.size, 0))})</button>
          <button className="btn" disabled={busy} onClick={run}>Actualiser</button>
        </div>
      </div>
      {error && <div className="card rowitem bad">Actualisation impossible : {error}. Les résultats affichés datent de la dernière recherche.</div>}
      {cats.map((cat) => {
        const rows = list.filter((c) => c.category === cat);
        return (
          <section key={cat} className="card">
            <header>
              <h3>{cat === "Installeurs téléchargés" ? "Installeurs anciens à vérifier" : cat}</h3>
              <span className="muted">{fmt(rows.reduce((s, c) => s + c.size, 0))}</span>
              <button className="link" onClick={() => setMany(rows, !rows.every((r) => selected.has(r.path)))}>tout cocher / décocher</button>
            </header>
            {rows.map((c) => (
              <label key={c.id} className="rowitem">
                <input type="checkbox" checked={selected.has(c.path)} onChange={() => toggle(c.path, c.size)} />
                <div className="grow">
                  <div className="ellipsis" title={c.path}>{c.path}</div>
                  <div className="muted small">{cat === "Installeurs téléchargés" ? "Installeur ancien dans Téléchargements ; installation du logiciel non vérifiée." : c.reason}</div>
                </div>
                <span className={"pill " + c.risk}>{RISK_LABEL[c.risk]}</span>
                <span className="r">{ageLabel(c.age_days)}</span>
                <b className="r">{fmt(c.size)}</b>
              </label>
            ))}
          </section>
        );
      })}
      {!list.length && <div className="empty"><p>Rien d'évident à nettoyer. Bien joué.</p></div>}
    </div>
  );
}

// ---------------- Doublons ----------------
export function Dups({ selected, toggle, setMany, drivesReady }: Sel & { drivesReady: boolean }) {
  const [groups, setGroups] = useState<DupGroup[] | null>(null);
  const [busy, setBusy] = useState(false);
  const [minMb, setMinMb] = useState(20);
  const [shown, setShown] = useState(100);
  const [err, setErr] = useState("");
  const [photosOnly, setPhotosOnly] = useState(false);
  const run = async () => {
    setBusy(true); setErr(""); setShown(100); setGroups(null);
    try { setGroups(await api.duplicates(minMb)); } catch (e) { setErr(String(e)); } finally { setBusy(false); }
  };
  if (!drivesReady) return <div className="empty"><h2>Analyse d'abord un disque</h2></div>;
  const visibleGroups = (groups ?? []).map((g) => {
    const paths = photosOnly ? g.paths.filter((p) => /\.(?:jpe?g|png|webp|heic|heif|avif|tiff?|bmp|dng|cr2|nef|arw)$/i.test(p)) : g.paths;
    return { ...g, paths, wasted: g.size * (paths.length - 1) };
  }).filter((g) => g.paths.length > 1);
  const wasted = visibleGroups.reduce((s, g) => s + g.wasted, 0);
  return (
    <div className="page">
      <div className="hero">
        <div>
          <div className="big">{groups ? fmt(wasted) : "Doublons"}</div>
          <div className="muted">{groups ? `${visibleGroups.length} groupes de fichiers identiques (hash vérifié)` : "Compare par taille, puis empreinte BLAKE3. Fichiers ≥ seuil, tous disques analysés."}</div>
        </div>
        <div className="row">
          <label className="muted">Seuil <input className="num" type="number" min={4} value={minMb} disabled={busy} onChange={(e) => { setMinMb(+e.target.value); setGroups(null); }} /> Mo</label>
          <label className="muted"><input type="checkbox" checked={photosOnly} onChange={(e) => setPhotosOnly(e.target.checked)} /> Photos uniquement</label>
          <button className="btn primary" disabled={busy} onClick={run}>{busy ? "Calcul des empreintes…" : "Chercher"}</button>
        </div>
      </div>
      {err && <div className="empty"><p>Erreur : {err}</p></div>}
      {visibleGroups.slice(0, shown).map((g, gi) => (
        <section key={gi} className="card">
          <header><h3>{fmt(g.size)} × {g.paths.length}</h3><span className="muted">gaspillé : {fmt(g.wasted)}</span>
            <button className="link" onClick={() => setMany(g.paths.slice(1).map((path) => ({ path, size: g.size })), true)}>garder le 1er, cocher les autres</button>
          </header>
          {g.paths.slice(0, 50).map((p) => (
            <label key={p} className="rowitem">
              <input type="checkbox" checked={selected.has(p)} onChange={() => toggle(p, g.size)} />
              <div className="grow ellipsis" title={p}>{p}</div>
            </label>
          ))}
          {g.paths.length > 50 && <div className="rowitem muted small">… et {g.paths.length - 50} autres copies</div>}
        </section>
      ))}
      {groups && visibleGroups.length > shown && (
        <button className="btn" onClick={() => setShown((n) => n + 100)}>Afficher 100 groupes de plus ({visibleGroups.length - shown} restants)</button>
      )}
      {groups && !visibleGroups.length && <div className="empty"><p>Aucun doublon {photosOnly ? "de photos " : ""}au-dessus de {minMb} Mo.</p></div>}
    </div>
  );
}

// ---------------- Assistant ----------------
export function Assistant({ addPicks, notify }: { addPicks: (p: Pick[]) => void; notify: (m: string) => void }) {
  const [msgs, setMsgs] = useState<(Msg & { picks?: Pick[] })[]>([]);
  const [text, setText] = useState("");
  const [busy, setBusy] = useState(false);
  const end = useRef<HTMLDivElement>(null);
  useEffect(() => end.current?.scrollIntoView({ behavior: "smooth" }), [msgs, busy]);

  const send = async (t: string) => {
    if (!t.trim() || busy) return;
    const next = [...msgs, { role: "user" as const, content: t }];
    setMsgs(next); setText(""); setBusy(true);
    try {
      const r = await api.chat(next.map(({ role, content }) => ({ role, content })));
      setMsgs([...next, { role: "assistant", content: r.message, picks: r.picks }]);
    } catch (e) { notify(String(e)); setMsgs(next); }
    setBusy(false);
  };
  const ideas = ["Libère 20 Go sans toucher à mes projets récents", "Qu'est-ce qui prend le plus de place pour rien ?", "Quels vieux projets puis-je alléger ?"];

  return (
    <div className="chat">
      <div className="msgs">
        {!msgs.length && (
          <div className="empty">
            <h2>Dis ce que tu veux libérer</h2>
            <p>L'IA lit la liste des éléments détectés (noms, tailles, dates — jamais le contenu) et te propose une sélection. Rien n'est supprimé sans ta validation.</p>
            <div className="chips">{ideas.map((i) => <button key={i} className="chip" onClick={() => send(i)}>{i}</button>)}</div>
          </div>
        )}
        {msgs.map((m, i) => (
          <div key={i} className={"msg " + m.role}>
            <div className="bubble">{m.content}</div>
            {m.picks && m.picks.length > 0 && (
              <div className="picks">
                {m.picks.map((p) => (
                  <div key={p.id} className="rowitem">
                    <div className="grow"><div className="ellipsis" title={p.path}>{p.path}</div><div className="muted small">{p.note}</div></div>
                    <b>{fmt(p.size)}</b>
                  </div>
                ))}
                <div className="row between">
                  <b>{fmt(m.picks.reduce((s, p) => s + p.size, 0))} proposés</b>
                  <button className="btn primary" onClick={() => { addPicks(m.picks!); notify("Ajouté à la sélection — vérifie puis supprime depuis la barre du bas."); }}>Ajouter à la sélection</button>
                </div>
              </div>
            )}
          </div>
        ))}
        {busy && <div className="msg assistant"><div className="bubble"><div className="pulse" /> Réflexion…</div></div>}
        <div ref={end} />
      </div>
      <form className="composer" onSubmit={(e) => { e.preventDefault(); send(text); }}>
        <input value={text} onChange={(e) => setText(e.target.value)} placeholder="Ex. : fais-moi 30 Go de place, garde mes modèles IA…" />
        <button className="btn primary" disabled={busy || !text.trim()}>Envoyer</button>
      </form>
    </div>
  );
}

// ---------------- Rapport ----------------
export function Report({ drives, notify }: { drives: string[]; notify: (m: string) => void }) {
  const [key, setKey] = useState(drives[0] ?? "C:");
  const [data, setData] = useState<ReportData | null>(null);
  const [text, setText] = useState("");
  const [busy, setBusy] = useState(false);
  useEffect(() => {
    setText(""); setData(null);
    api.reportData(key).then(setData).catch((e) => { setData({ key, history: [], prev_ts: null, cur_ts: null, growth: [] }); notify(String(e)); });
  }, [key]); // eslint-disable-line
  const gen = async () => { setBusy(true); try { setText(await api.report(key)); } catch (e) { notify(String(e)); } setBusy(false); };
  const hist = data?.history ?? [];
  const max = Math.max(1, ...hist.map((h) => h[1] - h[2]));
  return (
    <div className="page">
      <div className="hero">
        <div>
          <div className="big">Rapport {key}</div>
          <div className="muted">{hist.length} analyse(s) enregistrée(s). L'évolution apparaît dès la 2ᵉ analyse.</div>
        </div>
        <div className="row">
          <select value={key} onChange={(e) => setKey(e.target.value)}>{drives.map((d) => <option key={d}>{d}</option>)}</select>
          <button className="btn ai" disabled={busy || !hist.length} onClick={gen}>{busy ? "Rédaction…" : "✨ Générer le rapport"}</button>
        </div>
      </div>
      <Mensuel notify={notify} />
      {hist.length > 0 && (
        <section className="card"><header><h3>Espace utilisé</h3></header>
          <div className="spark">
            {hist.map((h, i) => <div key={i} title={`${new Date(h[0] * 1000).toLocaleString("fr")} · ${fmt(h[1] - h[2])}`} style={{ height: `${((h[1] - h[2]) / max) * 100}%` }} />)}
          </div>
        </section>
      )}
      {data && data.growth.length > 0 && (
        <section className="card"><header><h3>Plus fortes variations</h3></header>
          {data.growth.map((g) => (
            <div key={g.path} className="rowitem"><div className="grow ellipsis" title={g.path}>{g.path}</div>
              <b className={g.delta > 0 ? "up" : "down"}>{g.delta > 0 ? "+" : "−"}{fmt(Math.abs(g.delta))}</b></div>
          ))}
        </section>
      )}
      {data && !hist.length && <div className="empty"><h2>Aucune analyse enregistrée pour {key}</h2><p>Lance une analyse depuis l'Explorateur pour alimenter le rapport.</p></div>}
      {text && <section className="card report">{text.split("\n").map((l, i) => <p key={i}>{l}</p>)}</section>}
    </div>
  );
}
