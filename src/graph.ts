import type { GitCommit, GraphRow, GraphSymbol } from "./types";

const COLORS = [
  "#C9A227",
  "#499C54",
  "#C77DBB",
  "#D67C3A",
  "#548AF7",
  "#E06C75",
  "#56B6C2",
  "#98C379",
];

export function buildGraph(commits: GitCommit[]): GraphRow[] {
  const lanes: Array<string | null> = [];
  const rows: GraphRow[] = [];

  for (const commit of commits) {
    let lane = lanes.findIndex((hash) => hash === commit.hash);
    if (lane === -1) {
      lane = lanes.findIndex((hash) => hash === null);
      if (lane === -1) {
        lane = lanes.length;
        lanes.push(commit.hash);
      } else {
        lanes[lane] = commit.hash;
      }
    }

    const color = COLORS[lane % COLORS.length];
    const symbols: GraphSymbol[] = lanes.map((hash, index) => {
      if (index === lane) return "dot";
      if (hash) return "line";
      return "empty";
    });

    rows.push({
      lanes: symbols.map((symbol, index) => ({
        symbol,
        color: COLORS[index % COLORS.length],
      })),
      lane,
      color,
    });

    const parents = commit.parents;
    lanes[lane] = null;

    parents.forEach((parent, parentIndex) => {
      const existing = lanes.findIndex((hash) => hash === parent);
      if (existing !== -1) {
        if (parentIndex === 0 && existing !== lane) {
          // first parent already occupies another lane
        }
        return;
      }
      if (parentIndex === 0 && lanes[lane] === null) {
        lanes[lane] = parent;
        return;
      }
      const empty = lanes.findIndex((hash) => hash === null);
      if (empty === -1) {
        lanes.push(parent);
      } else {
        lanes[empty] = parent;
      }
    });
  }

  return rows;
}
