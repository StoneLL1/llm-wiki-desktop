import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";

import { Button } from "./button";

afterEach(cleanup);

describe("Button", () => {
  it("keeps translated action labels on one line without shrinking", () => {
    render(<Button>生成并预览当前知识卡片</Button>);

    expect(screen.getByRole("button")).toHaveClass(
      "whitespace-nowrap",
      "min-w-0",
      "shrink-0",
      "text-[13px]",
    );
  });
});
