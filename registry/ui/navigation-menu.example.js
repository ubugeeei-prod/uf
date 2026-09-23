// @flow
import * as NavigationMenu from "./navigation-menu.js";

/** A documentation site's navigation: two panels of links and a plain link. */
export component Example() {
  return (
    <NavigationMenu.Root aria-label="Main">
      <NavigationMenu.List>
        <NavigationMenu.Item value="guides">
          <NavigationMenu.Trigger>Guides</NavigationMenu.Trigger>
          <NavigationMenu.Content>
            <NavigationMenu.Link href="?section=install">Installation</NavigationMenu.Link>
            <NavigationMenu.Link href="?section=routing">Routing</NavigationMenu.Link>
          </NavigationMenu.Content>
        </NavigationMenu.Item>
        <NavigationMenu.Item value="reference">
          <NavigationMenu.Trigger>Reference</NavigationMenu.Trigger>
          <NavigationMenu.Content>
            <NavigationMenu.Link href="?section=cli">CLI</NavigationMenu.Link>
            <NavigationMenu.Link href="?section=config">Configuration</NavigationMenu.Link>
          </NavigationMenu.Content>
        </NavigationMenu.Item>
        <NavigationMenu.Item value="blog">
          <NavigationMenu.TopLink href="?section=blog">Blog</NavigationMenu.TopLink>
        </NavigationMenu.Item>
      </NavigationMenu.List>
    </NavigationMenu.Root>
  );
}
