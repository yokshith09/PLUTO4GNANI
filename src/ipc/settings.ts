// Generated types for settings (F0-07)
// TODO: auto-generate from Rust types with ts-rs

export type ThemeOption = "light" | "dark" | "system";
export type FontSize = "S" | "M" | "L";
export type PetSize = "S" | "M" | "L";

export interface Settings {
  "general.autostart": boolean;
  "general.theme": ThemeOption;
  "general.font_size": FontSize;
  "pet.visible": boolean;
  "pet.size": PetSize;
  "pet.opacity": number;
  "pet.always_on_top": boolean;
  "pet.typing": boolean;
  "pet.hover_card": boolean;
  /** Hide while another app fills the screen (a game, a film, a presentation). */
  "pet.hide_fullscreen": boolean;
  /** Play/pause/next buttons under the pet (off until turned on). */
  "pet.media": boolean;
  /** Installed pet pack shown instead of the bear (FR-63); null = the bear. */
  "pet.pack": string | null;
  "shortcuts.global_open": string | null;
  "shortcuts.global_new_note": string | null;
  "shortcuts.global_pet": string | null;
  "shortcuts.global_new_task": string | null;
  "tracking.apps": boolean;
  "tracking.exclude_apps": string[];
  "tracking.sites": boolean;
  /** macOS: also read Safari (can't tell private windows apart). */
  "tracking.safari": boolean;
  "advanced.log_level": "trace" | "debug" | "info" | "warn" | "error";
}

export type SettingsKey = keyof Settings;
