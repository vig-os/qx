import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { partsDescribe } from "../transport/fixtures";
import { mockTransport } from "../transport/mock";
import { TransportProvider } from "../data/TransportContext";
import { PrintPage } from "./PrintPage";

const maybeDescriptor = partsDescribe.collections[0];
if (!maybeDescriptor) throw new Error("parts fixture descriptor missing");
const descriptor = maybeDescriptor;

function renderPage() {
  return render(
    <TransportProvider transport={mockTransport()}>
      <PrintPage descriptor={descriptor} />
    </TransportProvider>,
  );
}

describe("PrintPage", () => {
  it("previews the SVGs returned by a Print dispatch for pasted ids", async () => {
    const { container } = renderPage();
    fireEvent.change(screen.getByLabelText("ids"), {
      target: { value: "PQ7G2MNVX4KH9T W3JD8RST2UVKXM" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Preview" }));
    await waitFor(() => {
      expect(container.querySelectorAll("svg")).toHaveLength(2);
    });
    expect(container.innerHTML).toContain("PQ7G2MNVX4KH9T");
    expect(screen.getByText(/2 labels/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Print" })).toBeEnabled();
  });

  it("surfaces protocol errors verbatim", async () => {
    renderPage();
    fireEvent.change(screen.getByLabelText("ids"), { target: { value: "PQ7" } });
    fireEvent.click(screen.getByRole("button", { name: "Preview" }));
    await screen.findByText(/BadRequest: query too short/);
  });
});
