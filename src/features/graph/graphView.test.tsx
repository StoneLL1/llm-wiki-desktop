import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import "../../i18n";
import { useGraphStore } from "../../stores/graphStore";
import { GraphView } from "./GraphView";

describe("GraphView", () => {
  it("mounts and renders the empty state when no graph data is loaded", () => {
    useGraphStore.getState().reset();
    // Neutralize the auto-load mount effect so the idle + no-data branch is
    // observable. The store's real load path is exercised in graphStore.test.ts.
    useGraphStore.setState({ load: async () => {} });
    render(<GraphView />);
    // No data + idle status → empty-state copy. Asserting the localized empty
    // text also confirms the component tree (and the sigma import) mounts
    // without throwing in a non-Tauri/jsdom environment.
    expect(screen.getByText(/No graph yet/i)).toBeInTheDocument();
  });
});
