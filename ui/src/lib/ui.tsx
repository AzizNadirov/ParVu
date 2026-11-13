// App-wide context: bootstrap data, translator, settings and dialog helpers.
import { createContext, useContext } from "react";
import type { ReactNode } from "react";
import type { Bootstrap, Settings } from "./api";
import type { Translate } from "./i18n";

export type Closer<T> = (value: T) => void;

export interface MessageButton {
  id: string;
  label: string;
  primary?: boolean;
  danger?: boolean;
}

export interface Ui {
  boot: Bootstrap;
  t: Translate;
  settings: Settings;
  setSettings: (s: Settings) => void;
  /** Show a modal; resolves with the value passed to `close`. Escape resolves with `dismissValue`. */
  showDialog<T>(render: (close: Closer<T>) => ReactNode, dismissValue: T): Promise<T>;
  alert(title: string, message: string, kind?: "info" | "warning" | "error"): Promise<void>;
  /** Resolves with the chosen button id, or "cancel" on Escape. */
  ask(title: string, message: string, buttons: MessageButton[]): Promise<string>;
  confirm(title: string, message: string): Promise<boolean>;
  prompt(title: string, label: string, initial?: string): Promise<string | null>;
  /** Short message in the status bar; cleared after `ms` when given. */
  status(text: string, ms?: number): void;
}

export const UiContext = createContext<Ui | null>(null);

export function useUi(): Ui {
  const ui = useContext(UiContext);
  if (!ui) throw new Error("useUi outside UiContext");
  return ui;
}
