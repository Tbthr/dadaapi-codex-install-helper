export type ActivationPhase =
  | "idle"
  | "detectingApp"
  | "fetchingProxyConfig"
  | "filteringProxyNodes"
  | "testingProxyNodes"
  | "selectingProxyNode"
  | "startingLocalProxy"
  | "savingNetworkState"
  | "writingLocale"
  | "stoppingDesktopApp"
  | "launchingDesktopApp"
  | "verifying"
  | "restoringNetwork"
  | "stoppingLocalProxy"
  | "succeeded"
  | "failed";

export interface ActivationEvent {
  phase: ActivationPhase;
  message: string;
  occurredAt: string;
}
