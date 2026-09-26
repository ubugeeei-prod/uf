// @flow
import * as Menu from "./menu.js";

/** A file menu: actions, a setting, a choice, a submenu and a disabled row. */
export component Example() {
  return (
    <Menu.Root>
      <Menu.Trigger>File</Menu.Trigger>
      <Menu.Content>
        <Menu.Group>
          <Menu.Label>Document</Menu.Label>
          <Menu.Item aria-keyshortcuts="Control+N">
            New file
            <Menu.Shortcut>Ctrl N</Menu.Shortcut>
          </Menu.Item>
          <Menu.Item disabled>Save</Menu.Item>
        </Menu.Group>
        <Menu.Separator />
        <Menu.CheckboxItem defaultChecked>Show ruler</Menu.CheckboxItem>
        <Menu.RadioGroup defaultValue="comfortable">
          <Menu.Label>Density</Menu.Label>
          <Menu.RadioItem value="compact">Compact</Menu.RadioItem>
          <Menu.RadioItem value="comfortable">Comfortable</Menu.RadioItem>
        </Menu.RadioGroup>
        <Menu.Separator />
        <Menu.Sub>
          <Menu.SubTrigger>Share</Menu.SubTrigger>
          <Menu.Content>
            <Menu.Item>Copy link</Menu.Item>
            <Menu.Item>Email</Menu.Item>
          </Menu.Content>
        </Menu.Sub>
      </Menu.Content>
    </Menu.Root>
  );
}
