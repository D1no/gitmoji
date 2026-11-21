import type { Meta, StoryObj } from "@storybook/react-vite";

const meta = {
  title: "Website",
  parameters: { layout: "fullscreen" },
} satisfies Meta;

export default meta;

export const Home: StoryObj = {
  render: () => (
    <iframe
      title="gitmoji.com"
      src="./site/index.html"
      style={{ display: "block", border: 0, width: "100%", height: "100dvh" }}
    />
  ),
};
