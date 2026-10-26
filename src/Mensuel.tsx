import { useCallback, useEffect, useState } from "react";
import { api, EtatMensuel, ResumeMensuel } from "./api";

function date(ts: number): string {
  return new Date(ts * 1000).toLocaleString("fr");
}

export default function Mensuel({ notify }: { notify: (message: string) => void }) {
  const [etat, setEtat] = useState<EtatMensuel | null>(null);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [seuil, setSeuil] = useState(10);
  const [busy, setBusy] = useState<"" | "load" | "generate" | "schedule" | "open">("");
  const [error, setError] = useState("");

  const charger = useCallback(async () => {
    setBusy("load"); setError("");
    try {
      const next = await api.mensuelEtat();
      setEtat(next);
      setSeuil(next.seuil_libre_pct);
      setSelectedId((id) => next.rapports.some((r) => r.id === id) ? id : next.rapports[0]?.id ?? null);
    } catch (e) { setError(String(e)); notify(String(e)); }
    finally { setBusy(""); }
  }, [notify]);

  useEffect(() => { void charger(); }, [charger]);

  const generer = async () => {
    setBusy("generate"); setError("");
    try {
      const rapport = await api.mensuelGenerer();
      setEtat((current) => current && {
        ...current,
        derniere_execution: rapport.ts,
        rapports: [rapport, ...current.rapports.filter((r) => r.id !== rapport.id)],
      });
      setSelectedId(rapport.id);
      notify("Rapport mensuel local créé");
    } catch (e) { setError(String(e)); notify(String(e)); }
    finally { setBusy(""); }
  };

  const planifier = async (active: boolean) => {
    if (!Number.isInteger(seuil) || seuil < 1 || seuil > 50) { setError("Choisis un seuil entre 1 et 50 %."); return; }
    if (active && !etat?.active && !window.confirm("Activer une tâche Windows le 1er de chaque mois à 9 h, lorsque ta session est ouverte ? Les rapports resteront sur cet ordinateur.")) return;
    setBusy("schedule"); setError("");
    try {
      const etaitActive = !!etat?.active;
      const next = await api.mensuelPlanifier(active, seuil);
      setEtat(next);
      setSeuil(next.seuil_libre_pct);
      notify(!etaitActive && !next.active ? "Seuil enregistré" : active ? "Planification mensuelle enregistrée" : "Planification mensuelle désactivée");
    } catch (e) { setError(String(e)); notify(String(e)); }
    finally { setBusy(""); }
  };

  const ouvrir = async (rapport: ResumeMensuel, format: "md" | "html" | "json") => {
    setBusy("open"); setError("");
    try { await api.mensuelOuvrir(rapport.id, format); }
    catch (e) { setError(String(e)); notify(String(e)); }
    finally { setBusy(""); }
  };

  const selection = etat?.rapports.find((r) => r.id === selectedId) ?? etat?.rapports[0];
  const seuilValide = Number.isInteger(seuil) && seuil >= 1 && seuil <= 50;
  const seuilModifie = !!etat && seuil !== etat.seuil_libre_pct;

  return (
    <section className="card" style={{ padding: 16 }}>
      <div className="between" style={{ alignItems: "flex-start" }}>
        <div>
          <h3>Rapport mensuel du PC</h3>
          <p className="muted small">Processus, stockage et éléments à vérifier. Les données restent locales : aucun e-mail ni appel IA. Le stockage réutilise les derniers scans disponibles et indique leur date ; aucun nouveau scan complet n'est lancé.</p>
        </div>
        <div className="row">
          <button className="btn" disabled={!!busy} onClick={charger}>Actualiser</button>
          <button className="btn primary" disabled={!!busy || !etat || seuilModifie || !seuilValide} onClick={generer}>{busy === "generate" ? "Création…" : "Générer maintenant"}</button>
        </div>
      </div>
      {seuilModifie && <p className="muted small">Enregistre le seuil avant de générer un rapport.</p>}

      {error && <div className="rowitem bad">Action impossible : {error} <button className="btn" disabled={!!busy} onClick={charger}>Actualiser l'état</button></div>}
      {!etat && !error && <p className="muted">Chargement du rapport mensuel…</p>}

      {etat && <>
        <div className="rowitem" style={{ cursor: "default", flexWrap: "wrap" }}>
          <div className="grow">
            <b>Planification {etat.active ? "active" : "désactivée"}</b>
            <div className="muted small">Le 1er de chaque mois à 9 h, session Windows ouverte. Option réversible. {etat.planification}</div>
            <div className="muted small">Dernière exécution : {etat.derniere_execution ? date(etat.derniere_execution) : "aucune"} · Dossier : {etat.dossier}</div>
          </div>
          <label className="muted small">Alerte si espace libre sous <input className="num" type="number" min={1} max={50} value={seuil} disabled={!!busy} onChange={(e) => setSeuil(Number(e.target.value))} /> %</label>
          <button className="btn" disabled={!!busy || !seuilValide || !seuilModifie} onClick={() => planifier(etat.active)}>Enregistrer le seuil</button>
          {etat.active ? <>
            <button className="btn danger" disabled={!!busy || !seuilValide} onClick={() => planifier(false)}>Désactiver</button>
          </> : <button className="btn primary" disabled={!!busy || !seuilValide} onClick={() => planifier(true)}>Activer la planification…</button>}
        </div>

        <div className="rowitem" style={{ cursor: "default", flexWrap: "wrap" }}>
          <label className="grow">Historique ({etat.rapports.length}) <select value={selection?.id ?? ""} disabled={!etat.rapports.length} onChange={(e) => setSelectedId(e.target.value)}>
            {etat.rapports.length ? etat.rapports.map((r) => <option key={r.id} value={r.id}>{date(r.ts)} · {r.alertes.length} alerte(s)</option>) : <option value="">Aucun rapport</option>}
          </select></label>
          {selection && <div className="row">
            <button className="btn" disabled={!!busy} onClick={() => ouvrir(selection, "md")}>Ouvrir Markdown</button>
            <button className="btn" disabled={!!busy} onClick={() => ouvrir(selection, "html")}>Ouvrir HTML</button>
            <button className="btn" disabled={!!busy} onClick={() => ouvrir(selection, "json")}>Ouvrir JSON</button>
          </div>}
        </div>
        {selection ? <div style={{ padding: "14px 0 2px" }}>
          {selection.alertes.length > 0 && <p className="warn">{selection.alertes.join(" · ")}</p>}
          <pre style={{ whiteSpace: "pre-wrap", overflowWrap: "anywhere", font: "inherit", lineHeight: 1.5 }}>{selection.markdown}</pre>
        </div> : <p className="muted">Aucun rapport encore créé. Le premier indiquera qu'aucune comparaison antérieure n'est disponible.</p>}
      </>}
    </section>
  );
}
