// @flow
import * as Menubar from "./menubar.js";

/** An editor's File, Edit and View menus. */
export component Example() {
  return (
    <Menubar.Root aria-label="Editor">
      <Menubar.Menu value="file">
        <Menubar.Trigger>File</Menubar.Trigger>
        <Menubar.Content>
          <Menubar.Item>New</Menubar.Item>
          <Menubar.Item>Open</Menubar.Item>
          <Menubar.Separator />
          <Menubar.Item disabled>Print</Menubar.Item>
        </Menubar.Content>
      </Menubar.Menu>
      <Menubar.Menu value="edit">
        <Menubar.Trigger>Edit</Menubar.Trigger>
        <Menubar.Content>
          <Menubar.Item>Undo</Menubar.Item>
          <Menubar.Item>Redo</Menubar.Item>
        </Menubar.Content>
      </Menubar.Menu>
      <Menubar.Menu value="view">
        <Menubar.Trigger>View</Menubar.Trigger>
        <Menubar.Content>
          <Menubar.CheckboxItem defaultChecked>Show toolbar</Menubar.CheckboxItem>
        </Menubar.Content>
      </Menubar.Menu>
    </Menubar.Root>
  );
}
