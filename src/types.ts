export interface UpdateInfo {
  current: string;
  latest: string;
  notes: string;
  url: string;
  available: boolean;
  autoInstall: boolean;
}

export interface AiConfig {
  baseUrl: string;
  apiKey: string;
  model: string;
}

export interface GeneratedMessage {
  message: string;
  source: string;
  model: string;
}

export interface RecentProject {
  path: string;
  name: string;
  lastOpened: number;
}

export interface SessionState {
  tabs: Array<{ path: string; name: string }>;
  activePath: string;
}

export interface ProjectTab {
  path: string;
  name: string;
  branch: string;
  dirty: boolean;
  merging: boolean;
}

export interface RepoInfo {
  path: string;
  name: string;
  branch: string;
  detached: boolean;
  ahead: number;
  behind: number;
  hasRemote: boolean;
  head: string;
  userName: string;
  isGit: boolean;
}

export interface GitFile {
  path: string;
  status: string;
  staged: boolean;
  untracked: boolean;
}

export interface GitCommit {
  hash: string;
  shortHash: string;
  message: string;
  author: string;
  date: string;
  timestamp: number;
  refs: string[];
  parents: string[];
}

export interface BranchInfo {
  name: string;
  current: boolean;
  remote: boolean;
  kind?: "local" | "remote" | "tag" | string;
  upstream?: string;
}

export interface RemoteDetail {
  name: string;
  url: string;
}

export interface RebaseItem {
  action: string;
  hash: string;
}

export interface StatusPayload {
  changes: GitFile[];
  unversioned: GitFile[];
}

export type GraphSymbol = "empty" | "line" | "dot" | "merge";

export interface GraphLane {
  color: string;
  symbol: GraphSymbol;
}

export interface FileDiff {
  path: string;
  binary: boolean;
  patch: string;
}

export interface FilePreview {
  path: string;
  content: string;
  binary: boolean;
  truncated: boolean;
  size: number;
}

export interface WorkspaceEntry {
  name: string;
  path: string;
  isDir: boolean;
  size: number;
}

export interface WorkspaceSearchOptions {
  regex?: boolean;
  caseSensitive?: boolean;
  include?: string[];
  maxResults?: number;
}

export interface WorkspaceSearchHit {
  path: string;
  line: number;
  column: number;
  text: string;
}

export interface WorkspaceSearchResult {
  hits: WorkspaceSearchHit[];
  truncated: boolean;
}

export interface DiffLine {
  kind: "hunk" | "add" | "del" | "ctx";
  text: string;
  oldNo: string;
  newNo: string;
}

export interface GraphRow {
  lanes: GraphLane[];
  lane: number;
  color: string;
}

export interface MergeFileInfo {
  path: string;
  oursChanged: boolean;
  theirsChanged: boolean;
  conflict: boolean;
  oursExists: boolean;
  baseExists: boolean;
  theirsExists: boolean;
  status: string;
}

export interface MergePreview {
  ours: string;
  theirs: string;
  base: string;
  baseShort: string;
  files: MergeFileInfo[];
}

export interface BlameLine {
  author: string;
  timestamp: number;
}

export interface MergeFileContent {
  path: string;
  ours: string;
  base: string;
  theirs: string;
  binary: boolean;
  tooLarge?: boolean;
  oursBlame?: BlameLine[];
  baseBlame?: BlameLine[];
  theirsBlame?: BlameLine[];
}

export interface ConflictFile {
  path: string;
  oursStatus: string;
  theirsStatus: string;
}

export interface MergeState {
  inProgress: boolean;
  ours: string;
  theirs: string;
  files: ConflictFile[];
}

export interface MergeOutcome {
  status: string;
  message: string;
  state: MergeState;
}

export interface MergeCompare {
  canMerge: boolean;
  message: string;
}

export interface PullOutcome {
  status: "ok" | "diverged";
  message: string;
  upstream: string;
}

export interface SyncOutcome {
  status: "ok" | "merged" | "conflicts";
  message: string;
  upstream: string;
  conflicts: string[];
}
