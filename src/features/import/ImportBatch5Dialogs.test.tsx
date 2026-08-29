import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { i18next } from "../../i18n";
import type {
  ImportCollectionPreview,
  RemoteMediaRetentionPlan,
} from "../../types/importV2Web";
import { ImportCollectionDialog } from "./ImportCollectionDialog";
import { ImportRemoteMediaDialog } from "./ImportRemoteMediaDialog";
import { ImportRestrictedContentDialog } from "./ImportRestrictedContentDialog";

beforeEach(async () => {
  await i18next.changeLanguage("en");
});

describe("Batch 5 import confirmations", () => {
  it("keeps collection discovery order while adding only selected children", async () => {
    const onConfirm = vi.fn().mockResolvedValue(undefined);
    const preview: ImportCollectionPreview = {
      taskId: "task-collection",
      collectionRef: "opaque-collection",
      sourceUrl: "https://space.bilibili.com/42/video",
      platform: "bilibili",
      title: "Saved videos",
      discoveredTotal: 3,
      loadedCount: 3,
      hasMore: false,
      nextCursor: null,
      totalDurationSeconds: 365,
      estimatedLoginCount: 1,
      estimatedAsrCount: 2,
      items: [
        {
          itemRef: "opaque-first",
          title: "First item",
          publicUrl: "https://www.bilibili.com/video/BV1first",
        },
        {
          itemRef: "opaque-second",
          title: "Second item",
          publicUrl: "https://www.bilibili.com/video/BV1second",
        },
        {
          itemRef: "opaque-third",
          title: "Third item",
          publicUrl: "https://www.bilibili.com/video/BV1third",
        },
      ],
    };

    render(
      <ImportCollectionDialog
        preview={preview}
        onLoadMore={vi.fn().mockResolvedValue(undefined)}
        onConfirm={onConfirm}
        onCancel={vi.fn()}
      />,
    );

    expect(screen.getByText("3 selected")).toBeInTheDocument();
    expect(screen.getByText("7m")).toBeInTheDocument();
    expect(screen.getByText("Estimated login")).toBeInTheDocument();
    expect(screen.getByText("Estimated ASR")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("checkbox", { name: /Second item/i }));
    expect(screen.getByText("2 selected")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Add selected" }));

    await waitFor(() => {
      expect(onConfirm).toHaveBeenCalledWith([
        "opaque-first",
        "opaque-third",
      ]);
    });
  });

  it("supports keyboard-reachable inversion and incremental loading for long collections", () => {
    const preview: ImportCollectionPreview = {
      taskId: "task-long-collection",
      collectionRef: "opaque-long-collection",
      sourceUrl: "https://www.bilibili.com/medialist/play/long",
      platform: "bilibili",
      title: "Long collection",
      discoveredTotal: 60,
      loadedCount: 30,
      hasMore: true,
      nextCursor: "opaque-cursor",
      totalDurationSeconds: null,
      estimatedLoginCount: 0,
      estimatedAsrCount: 30,
      items: Array.from({ length: 30 }, (_, index) => ({
        itemRef: `opaque-${index + 1}`,
        title: `Episode ${index + 1}`,
        publicUrl: `https://www.bilibili.com/video/BV${index + 1}`,
      })),
    };

    render(
      <ImportCollectionDialog
        preview={preview}
        onLoadMore={vi.fn().mockResolvedValue(undefined)}
        onConfirm={vi.fn().mockResolvedValue(undefined)}
        onCancel={vi.fn()}
      />,
    );

    expect(screen.queryByText("Episode 30")).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: /load more/i }));
    expect(screen.getByText("Episode 30")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Invert selection" }));
    expect(screen.getByText("0 selected")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Add selected" })).toBeDisabled();
  });

  it("selects newly loaded collection items without restoring an earlier deselection", () => {
    const firstPage: ImportCollectionPreview = {
      taskId: "task-paged-collection",
      collectionRef: "opaque-paged-collection",
      sourceUrl: "https://www.bilibili.com/medialist/play/paged",
      platform: "bilibili",
      title: "Paged collection",
      discoveredTotal: 3,
      loadedCount: 2,
      hasMore: true,
      nextCursor: "cursor-2",
      totalDurationSeconds: null,
      estimatedLoginCount: 0,
      estimatedAsrCount: 0,
      items: [
        {
          itemRef: "opaque-1",
          title: "Page item 1",
          publicUrl: "https://www.bilibili.com/video/BV1",
        },
        {
          itemRef: "opaque-2",
          title: "Page item 2",
          publicUrl: "https://www.bilibili.com/video/BV2",
        },
      ],
    };
    const props = {
      onLoadMore: vi.fn().mockResolvedValue(undefined),
      onConfirm: vi.fn().mockResolvedValue(undefined),
      onCancel: vi.fn(),
    };
    const view = render(<ImportCollectionDialog preview={firstPage} {...props} />);

    fireEvent.click(screen.getByRole("checkbox", { name: /Page item 1/i }));
    view.rerender(
      <ImportCollectionDialog
        preview={{
          ...firstPage,
          loadedCount: 3,
          hasMore: false,
          nextCursor: null,
          items: [
            ...firstPage.items,
            {
              itemRef: "opaque-3",
              title: "Page item 3",
              publicUrl: "https://www.bilibili.com/video/BV3",
            },
          ],
        }}
        {...props}
      />,
    );

    expect(screen.getByText("2 selected")).toBeInTheDocument();
    expect(screen.getByRole("checkbox", { name: /Page item 1/i })).not.toBeChecked();
    expect(screen.getByRole("checkbox", { name: /Page item 3/i })).toBeChecked();
  });

  it("hard-stops remote media retention when verified disk space is insufficient", () => {
    const onConfirm = vi.fn().mockResolvedValue(undefined);
    const plan: RemoteMediaRetentionPlan = {
      itemId: "item-1",
      estimatedBytes: 64 * 1024 ** 2,
      availableDiskBytes: 32 * 1024 ** 2,
      enoughDisk: false,
      quality: "best_available",
    };

    render(
      <ImportRemoteMediaDialog
        plan={plan}
        onConfirm={onConfirm}
        onCancel={vi.fn()}
      />,
    );

    const confirm = screen.getByRole("button", { name: "Save and retry" });
    expect(confirm).toBeDisabled();
    expect(screen.getByRole("alert")).toHaveTextContent(/available disk/i);
    expect(screen.getByText("64 MB")).toBeInTheDocument();
    expect(screen.getByRole("checkbox", {
      name: /reviewed the estimated size/i,
    })).toBeDisabled();
    fireEvent.click(confirm);
    expect(onConfirm).not.toHaveBeenCalled();
  });

  it("warns before the project's first restricted-content commit", async () => {
    const onConfirm = vi.fn().mockResolvedValue(undefined);
    render(
      <ImportRestrictedContentDialog
        open
        onConfirm={onConfirm}
        onCancel={vi.fn()}
      />,
    );

    expect(screen.getByText(/sharing this project may expose content/i)).toBeInTheDocument();
    expect(screen.getByText(/shown once for this project/i)).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Commit restricted content" }));
    await waitFor(() => expect(onConfirm).toHaveBeenCalledTimes(1));
  });
});
