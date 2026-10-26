import { useCallback, useState } from "react";
import { api, fmt, OdRacine, OdResult } from "./api";

export default function OneDrive({ notify }: { notify: (m: string) => void }) {
  const [racines, setRacines] = useState<OdRacine[] | null>(null);
  const [scanning, setScanning] = useState(false);
  const [sel, setSel] = useState<Set<string>>(new Set());
  const [epingles, setEpingles] = useState(false);
  const [confirm, setConfirm] = useState(false);
  const [busy, setBusy] = useState(false);
  const [res, setRes] = useState<OdResult[] | null>(null);

  const scan = useCallback(async () => {
    setScanning(true);
    setRes(null);
    try {
      setRacines(await api.onedriveScan());
      setSel(new Set());
    } catch (e) {
      notify(String(e));
    }
    setScanning(false);
  }, [notify]);

  const toggle = (p: string) =>
    setSel((s) => {
      const n = new Set(s);
      if (n.has(p)) n.delete(p);
      else n.add(p);
      return n;
    });
  const tous = racines?.flatMap((r) => r.dossiers) ?? [];
  const aLiberer = tous.filter((d) => sel.has(d.chemin));
  const octets = aLiberer.reduce((t, d) => t + d.local_octets - (epingles ? 0 : d.epingle_octets), 0);

  const go = async () => {
    setBusy(true);
    try {
      const result = await api.onedriveLiberer(aLiberer.map((d) => d.chemin), epingles);
      await scan();
      setRes(result);
    } catch (e) {
      notify(String(e));
    }
    setBusy(false);
    setConfirm(false);
  };

  return (
    <div className="page">
      <div className="hero">
        <div>
          <div className="big">OneDrive</div>
          <div className="muted">
            Libère l'espace local : les fichiers passent « en ligne uniquement » et restent dans le cloud (re-téléchargés à l'ouverture). Rien n'est supprimé.
          </div>
          {racines && (
            <div className="rs-count">
              {fmt(racines.reduce((t, r) => t + r.local_octets, 0))} présents sur ce disque · {racines.reduce((t, r) => t + r.cloud_fichiers, 0).toLocaleString("fr")} fichiers déjà en ligne uniquement
            </div>
          )}
        </div>
        <button className="btn primary" disabled={scanning} onClick={scan}>
          {scanning ? <><span className="pulse" /> Analyse…</> : "Analyser"}
        </button>
      </div>

      {racines && !racines.length && (
        <div className="empty">
          <h2>Aucun OneDrive détecté</h2>
          <p>Aucun dossier OneDrive n'est configuré pour ce compte Windows.</p>
        </div>
      )}

      {racines?.map((r) => (
        <section key={r.chemin} className="card">
          <header>
            <h3>{r.compte}</h3>
            <span className="muted ellipsis" title={r.chemin}>{r.chemin}</span>
            <b className="r" style={{ marginLeft: "auto" }}>{fmt(r.local_octets)}</b>
          </header>
          {r.dossiers.map((d) => (
            <label key={d.chemin} className="rowitem">
              <input type="checkbox" checked={sel.has(d.chemin)} disabled={d.local_octets === 0} onChange={() => toggle(d.chemin)} />
              <div className="grow">
                <div className="ellipsis" title={d.chemin}>{d.nom}</div>
                <div className="muted small">
                  {d.local_fichiers.toLocaleString("fr")} fichiers locaux · {d.cloud_fichiers.toLocaleString("fr")} en ligne uniquement
                  {d.epingle_octets > 0 ? ` · ${fmt(d.epingle_octets)} « toujours conserver »` : ""}
                </div>
              </div>
              <b className="r">{fmt(d.local_octets)}</b>
            </label>
          ))}
        </section>
      ))}

      {racines && racines.length > 0 && (
        <div className="card">
          <label className="rowitem">
            <input type="checkbox" checked={epingles} onChange={(e) => setEpingles(e.target.checked)} />
            <div className="grow">
              Inclure les fichiers « toujours conserver sur cet appareil »
              <div className="muted small">Désactivé : les fichiers que tu as épinglés sont ignorés.</div>
            </div>
          </label>
          <div className="rowitem" style={{ cursor: "default" }}>
            <div className="grow">
              <b>{aLiberer.length}</b> dossier{aLiberer.length > 1 ? "s" : ""} · jusqu'à <b>{fmt(octets)}</b> libérés
            </div>
            <button className="btn primary" disabled={!aLiberer.length || busy} onClick={() => setConfirm(true)}>Libérer l'espace…</button>
          </div>
        </div>
      )}

      {res && (
        <section className="card">
          <header>
            <h3>Résultat</h3>
            <span className="muted small">OneDrive libère l'espace en arrière-plan : cela peut prendre quelques minutes.</span>
          </header>
          {res.map((x) => (
            <div key={x.chemin} className={"rowitem" + (x.ok ? "" : " bad")} style={{ cursor: "default" }}>
              <span>{x.ok ? "✅" : "❌"}</span>
              <div className="grow">
                <div className="ellipsis" title={x.chemin}>{x.chemin}</div>
                <div className="small muted">{x.detail}</div>
              </div>
              <b className="r">{fmt(x.octets)}</b>
            </div>
          ))}
        </section>
      )}

      {confirm && (
        <div className="overlay" onClick={() => !busy && setConfirm(false)}>
          <div className="modal" onClick={(e) => e.stopPropagation()}>
            <h2>Libérer l'espace OneDrive</h2>
            <p className="warn">
              Les fichiers de {aLiberer.length} dossier(s) passeront « en ligne uniquement » (jusqu'à {fmt(octets)}). Ils restent dans le cloud, mais il faudra une connexion pour les ouvrir. Les fichiers non encore synchronisés ne sont libérés qu'après leur envoi.
            </p>
            <div className="list-box">
              {aLiberer.map((d) => (
                <div key={d.chemin} className="rowitem">
                  <div className="grow ellipsis" title={d.chemin}>{d.chemin}</div>
                  <b>{fmt(d.local_octets)}</b>
                </div>
              ))}
            </div>
            <div className="row between">
              <span />
              <div className="row">
                <button className="btn" disabled={busy} onClick={() => setConfirm(false)}>Annuler</button>
                <button className="btn danger" disabled={busy} onClick={go}>{busy ? "En cours…" : "Libérer"}</button>
              </div>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
