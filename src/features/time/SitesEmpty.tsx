import { Globe } from "lucide-react";
import { Switch } from "@/components/ui/switch";
import { useSettings } from "@/hooks/usePluto";
import { useSetting } from "@/hooks/useTheme";
import { isMac } from "@/lib/platform";

/** Browsers whose sites are recorded: Mac asks Chrome-family browsers through AppleScript, Windows reads the address bar. */
export const supportedBrowsers = (mac: boolean) => (mac ? "Chrome, Edge, Brave or Vivaldi" : "Chrome, Edge, Brave or Firefox");

/**
 * What to say when no website time is recorded yet. Site time is read from the browser's address
 * bar (no extension), behind its own switch, so the useful answer is either "turn it on" or
 * "browse a little".
 */
export function SitesEmpty({ compact = false }: { compact?: boolean }) {
  const { data: settings } = useSettings();
  const [, setSites] = useSetting("tracking.sites");
  const on = settings?.["tracking.sites"] === true;
  if (on) {
    return (
      <p className="text-xs leading-relaxed text-muted-foreground">
        No sites yet. Browse in {supportedBrowsers(isMac)} and the site names show up here. Private windows are never counted.{isMac && " Safari, Firefox and Arc aren’t recorded on Mac."}
      </p>
    );
  }
  return (
    <div className={compact ? "space-y-2.5" : "mx-auto flex max-w-sm flex-col items-center py-6 text-center"}>
      <span className="grid size-8 place-items-center rounded-lg bg-muted text-muted-foreground"><Globe className="size-4" /></span>
      <p className="text-xs leading-relaxed text-muted-foreground">
        Website time is off. When it is on, Pluto reads only the site name (like github.com) from your browser, never the full address.
      </p>
      <label className="flex items-center gap-2 text-xs font-medium">
        <Switch checked={false} onCheckedChange={(v) => void setSites(v)} aria-label="Turn on website tracking" />
        Track websites
      </label>
    </div>
  );
}
