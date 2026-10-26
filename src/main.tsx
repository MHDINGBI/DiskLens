import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import "./styles.css";

class Boundary extends React.Component<{ children: React.ReactNode }, { err: Error | null }> {
  state = { err: null as Error | null };
  static getDerivedStateFromError(err: Error) { return { err }; }
  render() {
    if (!this.state.err) return this.props.children;
    return (
      <div className="empty">
        <h2>Erreur d'affichage</h2>
        <p style={{ whiteSpace: "pre-wrap" }}>{String(this.state.err.stack || this.state.err)}</p>
        <button className="btn" onClick={() => this.setState({ err: null })}>Réessayer</button>
      </div>
    );
  }
}

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <Boundary>
      <App />
    </Boundary>
  </React.StrictMode>,
);
