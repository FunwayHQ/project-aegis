import { describe, it, expect } from "vitest";
import { render, screen } from "@testing-library/react";
import { StatCard } from "../src/components/StatCard";

describe("StatCard", () => {
  it("renders title and value", () => {
    render(<StatCard title="Test Title" value="123" />);

    expect(screen.getByText("Test Title")).toBeInTheDocument();
    expect(screen.getByText("123")).toBeInTheDocument();
  });

  it("renders subtitle when provided", () => {
    render(<StatCard title="Title" value="456" subtitle="Test subtitle" />);

    expect(screen.getByText("Test subtitle")).toBeInTheDocument();
  });

  it("does not render subtitle when not provided", () => {
    render(<StatCard title="Title" value="789" />);

    expect(screen.queryByText("Test subtitle")).not.toBeInTheDocument();
  });

  it("renders numeric value", () => {
    render(<StatCard title="Count" value={42} />);

    expect(screen.getByText("42")).toBeInTheDocument();
  });

  it("renders icon when provided", () => {
    render(
      <StatCard
        title="With Icon"
        value="100"
        icon={<span data-testid="test-icon">Icon</span>}
      />
    );

    expect(screen.getByTestId("test-icon")).toBeInTheDocument();
  });

  it("does not render icon container when icon not provided", () => {
    const { container } = render(<StatCard title="No Icon" value="200" />);

    // Check that the icon div with aegis class is not present
    const iconDiv = container.querySelector(".text-aegis-400");
    expect(iconDiv).not.toBeInTheDocument();
  });
});
