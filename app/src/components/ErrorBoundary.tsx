import { Component, type ErrorInfo, type ReactNode } from "react";

// Catches render errors in one screen so the rest of the window (navigation) keeps working.
export class ErrorBoundary extends Component<{ children: ReactNode }, { error: Error | null }> {
  state = { error: null as Error | null };

  static getDerivedStateFromError(error: Error) {
    return { error };
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    console.error(error, info.componentStack);
  }

  render() {
    if (!this.state.error) return this.props.children;
    return (
      <div>
        <h2>Er ging iets mis op dit scherm</h2>
        <p className="error">{this.state.error.message}</p>
        <p>Je gegevens zijn niet aangepast. Probeer het opnieuw of kies een ander scherm.</p>
        <button onClick={() => this.setState({ error: null })}>Probeer opnieuw</button>
      </div>
    );
  }
}
