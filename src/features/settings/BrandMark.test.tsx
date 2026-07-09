import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { BrandMark } from "./BrandMark";

describe("BrandMark", () => {
  it("renders the OpenClaw mascot glyph instead of a text placeholder", () => {
    const { container } = render(<BrandMark kind="openclaw" type="agent" />);
    const gradientIds = Array.from(container.querySelectorAll("linearGradient")).map((node) => node.id);
    const gradientFills = Array.from(container.querySelectorAll("path"))
      .map((node) => node.getAttribute("fill"))
      .filter((fill): fill is string => Boolean(fill?.startsWith("url(#openclaw-")));

    expect(container.querySelector("svg[viewBox='0 0 512 512']")).toBeInTheDocument();
    expect(gradientIds).toHaveLength(3);
    expect(new Set(gradientIds).size).toBe(3);
    expect(gradientIds.every((id) => id.startsWith("openclaw-"))).toBe(true);
    expect(gradientFills).toHaveLength(3);
    expect(screen.queryByText("OC")).not.toBeInTheDocument();
    expect(container.querySelector(".settings-brand__letter")).not.toBeInTheDocument();
  });

  it("renders the Hermes Agent official glyph instead of a text placeholder", () => {
    const { container } = render(<BrandMark kind="hermes" type="agent" />);

    expect(container.querySelector("svg[viewBox='0 0 16 16']")).toBeInTheDocument();
    expect(container.querySelector("path[d='M8 1.5v13']")).toBeInTheDocument();
    expect(container.querySelector("circle[cx='8'][cy='1.8'][r='1.1']")).toBeInTheDocument();
    expect(screen.queryByText("H")).not.toBeInTheDocument();
    expect(container.querySelector(".settings-brand__letter")).not.toBeInTheDocument();
  });

  it("uses a glyph instead of a text placeholder for custom BYOK providers", () => {
    const { container } = render(<BrandMark kind="custom" type="provider" />);

    expect(container.querySelector("svg[viewBox='0 0 24 24']")).toBeInTheDocument();
    expect(container.querySelector("path[d='M8 8.5 4.5 12 8 15.5']")).toBeInTheDocument();
    expect(container.querySelector("path[d='m16 8.5 3.5 3.5-3.5 3.5']")).toBeInTheDocument();
    expect(container.querySelector("path[d='m13.5 6.75-3 10.5']")).toBeInTheDocument();
    expect(screen.queryByText("API")).not.toBeInTheDocument();
    expect(container.querySelector(".settings-brand__letter")).not.toBeInTheDocument();
  });

  it("keeps unknown future brands on a generic glyph instead of initials", () => {
    const { container } = render(<BrandMark kind={"future" as never} type="agent" />);

    expect(container.querySelector("svg[viewBox='0 0 24 24']")).toBeInTheDocument();
    expect(container.querySelector("path[d='M12 4.5 18.5 8v8L12 19.5 5.5 16V8Z']")).toBeInTheDocument();
    expect(screen.queryByText("FU")).not.toBeInTheDocument();
    expect(container.querySelector(".settings-brand__letter")).not.toBeInTheDocument();
  });
});
