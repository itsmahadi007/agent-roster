// Mirrors roster-core's view structs (crates/roster-core/src/view.rs) and the app backend's
// PlanView. Fixtures in ./fixtures are written by roster-core's tests, so drift fails a test.

export type Tool = "claude" | "codex";

export interface WindowView {
  label: string;
  used_percent: number;
  hot: boolean;
}

export interface AgentView {
  id: string;
  name: string;
  tool: Tool;
  tool_label: string;
  color: string;
  email: string | null;
  plan: string | null;
  default_label: string | null;
  state: "ready" | "sign-in" | "working" | "not-installed";
  status_text: string;
  windows: WindowView[];
  reset_text: string | null;
  reset_hot: boolean;
  limits_note: string | null;
  claude_limits: boolean | null;
  live: boolean;
  tray_text: string;
  tray_hot: boolean;
}

export interface CallView {
  from_id: string;
  from_name: string;
  from_color: string;
  to_id: string;
  to_name: string;
  to_color: string;
  task: string | null;
  detail: string;
}

export interface WarningView {
  name: string;
  color: string;
  text: string;
}

export interface ColorView {
  label: string;
  hex: string;
}

export interface OverviewView {
  headline: string;
  agents: AgentView[];
  calls: CallView[];
  warnings: WarningView[];
  limits_updated: string | null;
  palette: ColorView[];
  next_color: string;
}

export interface CandidateView {
  tool: Tool;
  tool_label: string;
  installed: boolean;
  version: string | null;
  email: string | null;
  signed_in: boolean;
  suggested_name: string;
}

export interface FirstRunView {
  candidates: CandidateView[];
  prototype: boolean;
}

export interface ChangeView {
  id: number;
  summary: string;
  at: number;
  undone: boolean;
  color: string | null;
}

export interface StepView {
  kind: "add" | "change" | "remove" | "note";
  text: string;
}

export interface PlanView {
  token: number;
  summary: string;
  steps: StepView[];
}

export type OpRequest =
  | { op: "add"; name: string; tool: Tool; color: string | null; role: string | null; share: boolean }
  | { op: "adopt-all"; picks: [Tool, string][] }
  | { op: "import" }
  | { op: "enable-limits"; name: string }
  | { op: "disable-limits"; name: string }
  | { op: "rename"; name: string; new_name: string };

export type Screen = "agents" | "add" | "changes" | "first-run" | "help";
