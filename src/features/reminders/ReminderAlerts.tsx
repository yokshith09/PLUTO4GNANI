import { Button } from "@/components/ui/button";
import { Row, Section } from "@/features/settings/parts";
import { ipc } from "@/ipc";
import { notify } from "@/lib/notify";

/** How a due reminder reaches you, and the test button. Lives with the reminders on Today. */
export function ReminderAlerts() {
  return (
    <Section title="Reminder alerts">
      <Row label="System notification" hint="Nothing showed up? Allow notifications for Pluto and switch off Do not disturb in your system settings.">
        <div className="flex shrink-0 flex-col items-end gap-1.5">
          <Button
            size="sm" variant="secondary"
            onClick={() => void ipc.notificationTest().then(
              () => notify.info("Test sent. If no notification appeared, open the notification settings."),
              () => notify.error("Pluto couldn't show a notification."),
            )}
          >
            Send a test notification
          </Button>
          <Button size="sm" variant="ghost" onClick={() => void ipc.notificationSettingsOpen().catch(() => notify.error("Open your system settings to manage notifications."))}>
            Open notification settings
          </Button>
        </div>
      </Row>
    </Section>
  );
}
