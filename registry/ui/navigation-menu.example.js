// @flow
import {
  NavigationMenu,
  NavigationMenuContent,
  NavigationMenuItem,
  NavigationMenuLink,
  NavigationMenuList,
  NavigationMenuTopLink,
  NavigationMenuTrigger,
} from "./navigation-menu.js";

/** A documentation site's navigation: two panels of links and a plain link. */
export component Example() {
  return (
    <NavigationMenu aria-label="Main">
      <NavigationMenuList>
        <NavigationMenuItem value="guides">
          <NavigationMenuTrigger>Guides</NavigationMenuTrigger>
          <NavigationMenuContent>
            <NavigationMenuLink href="?section=install">Installation</NavigationMenuLink>
            <NavigationMenuLink href="?section=routing">Routing</NavigationMenuLink>
          </NavigationMenuContent>
        </NavigationMenuItem>
        <NavigationMenuItem value="reference">
          <NavigationMenuTrigger>Reference</NavigationMenuTrigger>
          <NavigationMenuContent>
            <NavigationMenuLink href="?section=cli">CLI</NavigationMenuLink>
            <NavigationMenuLink href="?section=config">Configuration</NavigationMenuLink>
          </NavigationMenuContent>
        </NavigationMenuItem>
        <NavigationMenuItem value="blog">
          <NavigationMenuTopLink href="?section=blog">Blog</NavigationMenuTopLink>
        </NavigationMenuItem>
      </NavigationMenuList>
    </NavigationMenu>
  );
}
