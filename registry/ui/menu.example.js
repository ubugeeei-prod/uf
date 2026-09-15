// @flow
import {
  Menu,
  MenuCheckboxItem,
  MenuContent,
  MenuGroup,
  MenuItem,
  MenuLabel,
  MenuRadioGroup,
  MenuRadioItem,
  MenuSeparator,
  MenuShortcut,
  MenuSub,
  MenuSubTrigger,
  MenuTrigger,
} from "./menu.js";

/** A file menu: actions, a setting, a choice, a submenu and a disabled row. */
export component Example() {
  return (
    <Menu>
      <MenuTrigger>File</MenuTrigger>
      <MenuContent>
        <MenuGroup>
          <MenuLabel>Document</MenuLabel>
          <MenuItem aria-keyshortcuts="Control+N">
            New file
            <MenuShortcut>Ctrl N</MenuShortcut>
          </MenuItem>
          <MenuItem disabled>Save</MenuItem>
        </MenuGroup>
        <MenuSeparator />
        <MenuCheckboxItem defaultChecked>Show ruler</MenuCheckboxItem>
        <MenuRadioGroup defaultValue="comfortable">
          <MenuLabel>Density</MenuLabel>
          <MenuRadioItem value="compact">Compact</MenuRadioItem>
          <MenuRadioItem value="comfortable">Comfortable</MenuRadioItem>
        </MenuRadioGroup>
        <MenuSeparator />
        <MenuSub>
          <MenuSubTrigger>Share</MenuSubTrigger>
          <MenuContent>
            <MenuItem>Copy link</MenuItem>
            <MenuItem>Email</MenuItem>
          </MenuContent>
        </MenuSub>
      </MenuContent>
    </Menu>
  );
}
