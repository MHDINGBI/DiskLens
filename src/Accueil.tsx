import type { Drive } from "./api";
import { fmt } from "./api";
import Icon, { IconName } from "./Icon";
const actions: { route: string; icon: IconName; title: string; text: string }[] = [
  { route: "logiciels", icon: "package", title: "Désinstaller un logiciel", text: "Retrouver une application, la désinstaller et examiner ses restes." },
  { route: "clean", icon: "clean", title: "Libérer de l’espace", text: "Examiner les caches et les fichiers proposés au nettoyage." },
  { route: "explorer", icon: "disk", title: "Analyser le stockage", text: "Voir les dossiers et les fichiers qui occupent vos disques." },
  { route: "residus", icon: "residue", title: "Trouver des résidus", text: "Rechercher les traces d’anciens logiciels, même sans connaître leur nom." },
];
export default function Accueil({ drives, loading, error, refresh }: { drives: Drive[]; loading: boolean; error: string; refresh: () => void }) {
  return <div className="page home-page">
    <section className="home-intro"><div><p className="section-caption">Votre ordinateur</p><h2>Par où souhaitez-vous commencer ?</h2><p className="muted">Choisissez une action. Les éléments à nettoyer vous seront présentés avant toute suppression.</p></div><span className="home-emblem"><Icon name="disk" size={38} /></span></section>
    <div className="task-grid">{actions.map((a) => <a className="task-card" href={`#${a.route}`} key={a.route}><span className="task-icon"><Icon name={a.icon} size={24} /></span><div><h3>{a.title}</h3><p>{a.text}</p></div><Icon name="arrow" size={18} /></a>)}</div>
    <section className="card storage-overview"><header><div className="grow"><h2>Vos disques</h2><p className="muted small">Espace disponible actuellement</p></div><button className="btn" disabled={loading} onClick={refresh}><Icon name="refresh" size={16} />Actualiser</button></header>
      {error ? <div className="inline-message error" role="alert">{error} Vous pouvez réessayer avec Actualiser.</div> : loading && !drives.length ? <div className="loading-state" role="status"><span className="pulse" />Lecture des volumes…</div> : !drives.length ? <div className="empty compact"><p>Aucun disque disponible.</p></div> : drives.map((d) => {
        const pct = d.total > 0 ? Math.max(0, Math.min(100, (d.total - d.free) / d.total * 100)) : 0;
        return <div className="volume-row" key={d.key}><div className="volume-letter">{d.key}</div><div className="volume-info"><div className="between"><strong>{d.label || "Disque local"}</strong><span className="muted small">{Math.round(pct)} % utilisé</span></div><div className="meter" role="meter" aria-label={`Occupation de ${d.key}`} aria-valuenow={Math.round(pct)} aria-valuemin={0} aria-valuemax={100}><i className={pct > 90 ? "hot" : pct > 75 ? "warm" : ""} style={{ width: `${pct}%` }} /></div><span className="muted small">{fmt(d.total - d.free)} utilisés sur {fmt(d.total)}</span></div><div className="volume-free"><strong>{fmt(d.free)}</strong><span>disponibles</span></div></div>;
      })}
    </section>
    <div className="home-secondary"><div><Icon name="heart" /><div><h3>Un PC plus fluide</h3><p className="muted">Processus, démarrage et état des protections.</p></div></div><a className="btn" href="#sante">Ouvrir la santé du PC<Icon name="arrow" size={16} /></a></div>
  </div>;
}
