import { Component, type ErrorInfo, type ReactNode } from "react";
import { Button } from "@/components/ui/button";

interface Props {
  children: ReactNode;
  /** The pet window is a small transparent strip: it retries by itself instead of showing a panel. */
  quiet?: boolean;
}
interface State { failed: boolean; tries: number }

/**
 * A screen that crashes must not blank the whole window. The data is untouched (it lives in the
 * database, not on screen), so the person can try again or reload. In the pet window it quietly
 * tries again up to three times, so a one-off glitch does not leave the pet gone.
 */
export class ErrorBoundary extends Component<Props, State> {
  override state: State = { failed: false, tries: 0 };
  private timer: number | undefined;

  static getDerivedStateFromError(): Partial<State> {
    return { failed: true };
  }

  override componentDidCatch(error: Error, info: ErrorInfo) {
    console.error("Pluto screen crashed:", error, info.componentStack);
    if (this.props.quiet && this.state.tries < 3) {
      this.timer = window.setTimeout(() => this.setState((s) => ({ failed: false, tries: s.tries + 1 })), 2000);
    }
  }

  override componentWillUnmount() {
    window.clearTimeout(this.timer);
  }

  override render() {
    if (!this.state.failed) return this.props.children;
    if (this.props.quiet) return null;
    return (
      <div role="alert" className="mx-auto mt-24 max-w-sm space-y-3 rounded-xl border bg-card p-6 text-center">
        <p className="text-sm font-semibold">Something went wrong on this screen</p>
        <p className="text-xs text-muted-foreground">Nothing is lost: your tasks, notes and settings are saved on this computer.</p>
        <div className="flex justify-center gap-2">
          <Button size="sm" variant="outline" onClick={() => this.setState({ failed: false })}>Try again</Button>
          <Button size="sm" onClick={() => window.location.reload()}>Reload Pluto</Button>
        </div>
      </div>
    );
  }
}
