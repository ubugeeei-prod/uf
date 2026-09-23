//! The copies `uf ui add` wrote, moved to the registry's one-name-per-component
//! shape (ubugeeei-prod/uf#1453).
//!
//! # What changed
//!
//! The registry's components exported every part under a prefixed name —
//! `dialog.js` exported `Dialog`, `DialogTrigger`, `DialogContent` — and now
//! export them unprefixed, `Root`, `Trigger`, `Content`, for a page to import
//! as `import * as Dialog from "./components/ui/dialog.js"`. A copy is the
//! project's file, and it keeps the old shape until the project changes it; the
//! pages that import it keep working until then. This is that change, made
//! for them:
//!
//! * [`reshape`] gives a copy the registry's new shape: each part declared
//!   under its full name and exported under the short one, the headless parts
//!   imported by name rather than through `import * as Primitive`, and the
//!   part names in its comments spelled the new way. On a copy nobody edited,
//!   the result — formatted, as the caller formats it — is the registry's text
//!   at the release that changed its shape, byte for byte (all fifty-seven were
//!   checked when it did). So a later `uf ui update` sees the same change on both
//!   sides and conflicts on nothing; the tests hold twelve components to that
//!   against this registry, whatever else it has changed since.
//! * Every import of a copy is rewritten by [`super::ui_namespaces`]'s engine
//!   with [`Origin::Copy`](super::ui_namespaces::Origin::Copy):
//!   `import { Dialog, DialogTrigger } from "./components/ui/dialog.js"`
//!   becomes `import * as Dialog from "./components/ui/dialog.js"`, and
//!   `<Dialog>` and `<DialogTrigger>` become `<Dialog.Root>` and
//!   `<Dialog.Trigger>`.
//!
//! # How a copy is recognised
//!
//! By its stamp, the line `uf ui add` appends, which names the component. A
//! file without one is not a copy, whatever it is called, and nothing here
//! touches it. The stamp itself is kept: it records the registry text the copy
//! began as, which is what makes the copy read as edited now and lets
//! `uf ui update` merge onto it.
//!
//! # What is left for a person
//!
//! An import of a copy through anything but a relative path (an alias such as
//! `@/components/ui/dialog.js`) is not resolved, so it is not rewritten; and
//! a copy edited so far that its old exports are no longer where the
//! registry put them is reshaped only as far as it still matches. Both surface
//! as Flow errors naming the missing export, and the report lists every file
//! the engine refused.

use std::collections::BTreeMap;

use uf_flow::scan::TokenKind;
use uf_ui::Stamp;
use uf_ui::registry::namespace_name;

use super::ui_namespaces::part;

/// The catalog ID `tools/codemods/catalog.json` registers this migration under.
pub(super) const UI_COPIES: &str = "ui-registry-namespaces-1453";

/// Every registry component with more than one part, and each name its file
/// exported before ubugeeei-prod/uf#1453 with the part that name is now.
///
/// Frozen for the reason [`super::ui_namespaces`]'s `PARTS` is: the registry
/// this uf embeds no longer says what the old names were. `context-menu` and
/// `menubar` re-exported `menu`'s parts under their own prefix, and those
/// names are here too.
const OLD: &[(&str, &[(&str, &str)])] = &[
    (
        "accordion",
        &[
            ("Accordion", "Root"),
            ("AccordionItem", "Item"),
            ("AccordionTrigger", "Trigger"),
            ("AccordionContent", "Content"),
        ],
    ),
    (
        "alert-dialog",
        &[
            ("AlertDialog", "Root"),
            ("AlertDialogTrigger", "Trigger"),
            ("AlertDialogContent", "Content"),
            ("AlertDialogHeader", "Header"),
            ("AlertDialogFooter", "Footer"),
            ("AlertDialogTitle", "Title"),
            ("AlertDialogDescription", "Description"),
            ("AlertDialogAction", "Action"),
            ("AlertDialogCancel", "Cancel"),
        ],
    ),
    (
        "alert",
        &[
            ("Alert", "Root"),
            ("AlertTitle", "Title"),
            ("AlertDescription", "Description"),
        ],
    ),
    (
        "avatar",
        &[
            ("Avatar", "Root"),
            ("AvatarImage", "Image"),
            ("AvatarFallback", "Fallback"),
        ],
    ),
    (
        "breadcrumb",
        &[
            ("Breadcrumb", "Root"),
            ("BreadcrumbList", "List"),
            ("BreadcrumbItem", "Item"),
            ("BreadcrumbLink", "Link"),
            ("BreadcrumbPage", "Page"),
            ("BreadcrumbSeparator", "Separator"),
        ],
    ),
    (
        "calendar",
        &[
            ("Calendar", "Root"),
            ("CalendarHeader", "Header"),
            ("CalendarPrevious", "Previous"),
            ("CalendarNext", "Next"),
            ("CalendarMonth", "Month"),
        ],
    ),
    (
        "card",
        &[
            ("Card", "Root"),
            ("CardHeader", "Header"),
            ("CardTitle", "Title"),
            ("CardDescription", "Description"),
            ("CardContent", "Content"),
            ("CardFooter", "Footer"),
        ],
    ),
    (
        "carousel",
        &[
            ("Carousel", "Root"),
            ("CarouselContent", "Content"),
            ("CarouselItem", "Item"),
            ("CarouselControls", "Controls"),
            ("CarouselPrevious", "Previous"),
            ("CarouselNext", "Next"),
            ("CarouselPause", "Pause"),
        ],
    ),
    (
        "collapsible",
        &[
            ("Collapsible", "Root"),
            ("CollapsibleTrigger", "Trigger"),
            ("CollapsibleContent", "Content"),
        ],
    ),
    (
        "color-picker",
        &[
            ("ColorPicker", "Root"),
            ("ColorPickerInput", "Input"),
            ("ColorPickerField", "Field"),
            ("ColorPickerChannel", "Channel"),
            ("ColorPickerSwatch", "Swatch"),
        ],
    ),
    (
        "combobox",
        &[
            ("Combobox", "Root"),
            ("ComboboxLabel", "Label"),
            ("ComboboxInput", "Input"),
            ("ComboboxList", "List"),
            ("ComboboxOption", "Option"),
            ("ComboboxGroup", "Group"),
            ("ComboboxGroupLabel", "GroupLabel"),
            ("ComboboxEmpty", "Empty"),
            ("ComboboxStatus", "Status"),
        ],
    ),
    (
        "context-menu",
        &[
            ("ContextMenu", "Root"),
            ("ContextMenuTrigger", "Trigger"),
            ("ContextMenuCheckboxItem", "CheckboxItem"),
            ("ContextMenuContent", "Content"),
            ("ContextMenuGroup", "Group"),
            ("ContextMenuItem", "Item"),
            ("ContextMenuLabel", "Label"),
            ("ContextMenuRadioGroup", "RadioGroup"),
            ("ContextMenuRadioItem", "RadioItem"),
            ("ContextMenuSeparator", "Separator"),
            ("ContextMenuShortcut", "Shortcut"),
            ("ContextMenuSub", "Sub"),
            ("ContextMenuSubTrigger", "SubTrigger"),
        ],
    ),
    (
        "date-picker",
        &[
            ("DatePicker", "Root"),
            ("DatePickerGroup", "Group"),
            ("DatePickerInput", "Input"),
            ("DatePickerTrigger", "Trigger"),
            ("DatePickerContent", "Content"),
        ],
    ),
    (
        "date-range-picker",
        &[
            ("DateRangePicker", "Root"),
            ("DateRangePickerStartField", "StartField"),
            ("DateRangePickerEndField", "EndField"),
            ("DateRangePickerTrigger", "Trigger"),
            ("DateRangePickerCalendar", "Calendar"),
        ],
    ),
    (
        "dialog",
        &[
            ("Dialog", "Root"),
            ("DialogTrigger", "Trigger"),
            ("DialogContent", "Content"),
            ("DialogHeader", "Header"),
            ("DialogFooter", "Footer"),
            ("DialogTitle", "Title"),
            ("DialogDescription", "Description"),
            ("DialogClose", "Close"),
        ],
    ),
    (
        "drawer",
        &[
            ("Drawer", "Root"),
            ("DrawerTrigger", "Trigger"),
            ("DrawerContent", "Content"),
            ("DrawerHeader", "Header"),
            ("DrawerFooter", "Footer"),
            ("DrawerTitle", "Title"),
            ("DrawerDescription", "Description"),
            ("DrawerClose", "Close"),
        ],
    ),
    (
        "field",
        &[
            ("Field", "Root"),
            ("FieldLabel", "Label"),
            ("FieldInput", "Input"),
            ("FieldTextarea", "Textarea"),
            ("FieldDescription", "Description"),
            ("FieldStatus", "Status"),
            ("FieldError", "Error"),
        ],
    ),
    (
        "hover-card",
        &[
            ("HoverCard", "Root"),
            ("HoverCardTrigger", "Trigger"),
            ("HoverCardContent", "Content"),
        ],
    ),
    (
        "input-otp",
        &[
            ("InputOtp", "Root"),
            ("InputOtpGroup", "Group"),
            ("InputOtpSlot", "Slot"),
            ("InputOtpSeparator", "Separator"),
        ],
    ),
    (
        "menu",
        &[
            ("Menu", "Root"),
            ("MenuTrigger", "Trigger"),
            ("MenuContent", "Content"),
            ("MenuItem", "Item"),
            ("MenuCheckboxItem", "CheckboxItem"),
            ("MenuRadioGroup", "RadioGroup"),
            ("MenuRadioItem", "RadioItem"),
            ("MenuGroup", "Group"),
            ("MenuLabel", "Label"),
            ("MenuSeparator", "Separator"),
            ("MenuShortcut", "Shortcut"),
            ("MenuSub", "Sub"),
            ("MenuSubTrigger", "SubTrigger"),
        ],
    ),
    (
        "menubar",
        &[
            ("Menubar", "Root"),
            ("MenubarMenu", "Menu"),
            ("MenubarTrigger", "Trigger"),
            ("MenubarCheckboxItem", "CheckboxItem"),
            ("MenubarContent", "Content"),
            ("MenubarGroup", "Group"),
            ("MenubarItem", "Item"),
            ("MenubarLabel", "Label"),
            ("MenubarRadioGroup", "RadioGroup"),
            ("MenubarRadioItem", "RadioItem"),
            ("MenubarSeparator", "Separator"),
            ("MenubarShortcut", "Shortcut"),
            ("MenubarSub", "Sub"),
            ("MenubarSubTrigger", "SubTrigger"),
        ],
    ),
    (
        "navigation-menu",
        &[
            ("NavigationMenu", "Root"),
            ("NavigationMenuList", "List"),
            ("NavigationMenuItem", "Item"),
            ("NavigationMenuTrigger", "Trigger"),
            ("NavigationMenuContent", "Content"),
            ("NavigationMenuLink", "Link"),
            ("NavigationMenuTopLink", "TopLink"),
        ],
    ),
    (
        "number-field",
        &[
            ("NumberField", "Root"),
            ("NumberFieldInput", "Input"),
            ("NumberFieldIncrement", "Increment"),
            ("NumberFieldDecrement", "Decrement"),
        ],
    ),
    (
        "pagination",
        &[
            ("Pagination", "Root"),
            ("PaginationContent", "Content"),
            ("PaginationItem", "Item"),
            ("PaginationPrevious", "Previous"),
            ("PaginationNext", "Next"),
        ],
    ),
    (
        "popover",
        &[
            ("Popover", "Root"),
            ("PopoverTrigger", "Trigger"),
            ("PopoverContent", "Content"),
        ],
    ),
    (
        "radio-group",
        &[("RadioGroup", "Root"), ("RadioGroupItem", "Item")],
    ),
    (
        "resizable",
        &[
            ("ResizablePanelGroup", "PanelGroup"),
            ("ResizablePanel", "Panel"),
            ("ResizableHandle", "Handle"),
        ],
    ),
    (
        "select",
        &[
            ("Select", "Root"),
            ("SelectLabel", "Label"),
            ("SelectTrigger", "Trigger"),
            ("SelectValue", "Value"),
            ("SelectList", "List"),
            ("SelectOption", "Option"),
            ("SelectGroup", "Group"),
            ("SelectGroupLabel", "GroupLabel"),
            ("SelectSeparator", "Separator"),
        ],
    ),
    (
        "sheet",
        &[
            ("Sheet", "Root"),
            ("SheetTrigger", "Trigger"),
            ("SheetContent", "Content"),
            ("SheetHeader", "Header"),
            ("SheetFooter", "Footer"),
            ("SheetTitle", "Title"),
            ("SheetDescription", "Description"),
            ("SheetClose", "Close"),
        ],
    ),
    (
        "sidebar",
        &[
            ("Sidebar", "Root"),
            ("SidebarContent", "Content"),
            ("SidebarHeader", "Header"),
            ("SidebarFooter", "Footer"),
            ("SidebarItem", "Item"),
            ("SidebarTrigger", "Trigger"),
        ],
    ),
    ("skeleton", &[("Skeleton", "Root"), ("SkeletonBox", "Box")]),
    (
        "table",
        &[
            ("Table", "Root"),
            ("TableCaption", "Caption"),
            ("TableHeader", "Header"),
            ("TableBody", "Body"),
            ("TableRow", "Row"),
            ("TableHead", "Head"),
            ("TableCell", "Cell"),
            ("TableRowHeader", "RowHeader"),
            ("TableSelectAll", "SelectAll"),
            ("TableRowSelect", "RowSelect"),
        ],
    ),
    (
        "tabs",
        &[
            ("Tabs", "Root"),
            ("TabsList", "List"),
            ("TabsTab", "Tab"),
            ("TabsPanel", "Panel"),
        ],
    ),
    (
        "toast",
        &[
            ("Toaster", "Region"),
            ("Toast", "Root"),
            ("ToastTitle", "Title"),
            ("ToastDescription", "Description"),
            ("ToastAction", "Action"),
            ("ToastClose", "Close"),
        ],
    ),
    (
        "toggle-group",
        &[("ToggleGroup", "Root"), ("ToggleGroupItem", "Item")],
    ),
    (
        "tooltip",
        &[
            ("TooltipProvider", "Provider"),
            ("Tooltip", "Root"),
            ("TooltipTrigger", "Trigger"),
            ("TooltipContent", "Content"),
        ],
    ),
];

/// `@uniflowed/ui`'s components that are one name rather than a namespace.
const HEADLESS_SINGLE: &[&str] = &[
    "Checkbox",
    "DateField",
    "GridList",
    "I18nProvider",
    "ListBox",
    "Progress",
    "Separator",
    "Switch",
    "TagGroup",
    "TimeField",
    "Toggle",
    "Tree",
    "VisuallyHidden",
];

/// The part a name a copy of `component` used to export is now, or `None`.
pub(super) fn old_part(component: &str, name: &str) -> Option<&'static str> {
    OLD.iter()
        .find(|(each, _)| *each == component)?
        .1
        .iter()
        .find(|(old, _)| *old == name)
        .map(|(_, part)| *part)
}

/// The name a part is declared under in the new registry: the old name, except
/// the root, whose old name is the namespace's and becomes `DialogRoot`.
fn local_name(component: &str, old: &str) -> String {
    let namespace = namespace_name(component);
    if old == namespace {
        format!("{namespace}Root")
    } else {
        old.to_owned()
    }
}

/// Every copy among `files`, by path, with the component its stamp names —
/// one this uf's registry still has.
///
/// A component with one part exported one name before and after, so no page
/// importing it changes; its copy is still given the registry's new way of
/// importing the headless parts, by name, so that it too matches the registry.
pub(super) fn copies(files: &[(String, String)]) -> BTreeMap<String, &'static str> {
    let Ok(registry) = uf_ui::Registry::embedded() else {
        return BTreeMap::new();
    };
    let mut found = BTreeMap::new();
    for (path, text) in files {
        let Some(stamp) = text
            .lines()
            .rev()
            .find(|line| !line.trim().is_empty())
            .and_then(Stamp::parse)
        else {
            continue;
        };
        if let Some(component) = registry.get(stamp.component.as_str()) {
            found.insert(path.clone(), component.name);
        }
    }
    found
}

/// The copy a relative specifier written in `importer` leads to, if it leads
/// to one: `./components/ui/dialog.js` from `app/$page.js` is
/// `app/components/ui/dialog.js`, with or without its extension.
pub(super) fn resolve(
    importer: &str,
    specifier: &str,
    copies: &BTreeMap<String, &'static str>,
) -> Option<&'static str> {
    if !(specifier.starts_with("./") || specifier.starts_with("../")) {
        return None;
    }
    let mut parts: Vec<&str> = importer.split('/').collect();
    parts.pop();
    for segment in specifier.split('/') {
        match segment {
            "." | "" => {}
            ".." => {
                parts.pop()?;
            }
            other => parts.push(other),
        }
    }
    let joined = parts.join("/");
    copies
        .get(&joined)
        .or_else(|| copies.get(&format!("{joined}.js")))
        .copied()
}

/// A copy of `component` in the registry's new shape, `Ok(None)` when it
/// already has it, or the reason it was left alone. See the module header.
pub(super) fn reshape(source: &str, component: &'static str) -> Result<Option<String>, String> {
    let Some((_, old)) = OLD.iter().find(|(each, _)| *each == component) else {
        // One part: only the headless import changes shape.
        let named = name_headless_imports(source)?;
        return Ok((named != source).then_some(named));
    };
    // The stamp stays the last line; everything below happens above it.
    let (source, stamp) = match source.trim_end().rfind('\n') {
        Some(at) if Stamp::parse(source[at + 1..].trim_end()).is_some() => source.split_at(at + 1),
        _ => (source, ""),
    };
    let namespace = namespace_name(component);
    let declared: Vec<(&str, &str)> = old
        .iter()
        .filter(|(name, _)| source.contains(&format!("\nexport component {name}(")))
        .copied()
        .collect();
    let mut text = source.to_owned();

    // `context-menu` and `menubar`: the menu parts they handed on under their
    // own prefix are handed on from `menu.js` under the menu's names.
    let handed_on = rehand_menu_parts(&mut text, component, &namespace);
    if declared.is_empty() && !handed_on {
        return Ok(None);
    }

    // The root takes its full name, everywhere the file refers to it.
    let renames: Vec<(&str, String)> = declared
        .iter()
        .map(|(name, _)| (*name, local_name(component, name)))
        .filter(|(name, local)| name != local)
        .collect();
    if !renames.is_empty() {
        text = rename_identifiers(&text, &renames);
    }
    for (name, _) in &declared {
        text = text.replace(
            &format!("\nexport component {}(", local_of(&renames, name)),
            &format!("\ncomponent {}(", local_of(&renames, name)),
        );
    }

    text = name_headless_imports(&text)?;

    // The part names in the file's own prose, the way a page now writes them.
    text = text
        .split_inclusive('\n')
        .map(|line| {
            let trimmed = line.trim_start();
            if !(trimmed.starts_with("//") || trimmed.starts_with('*') || trimmed.starts_with("/*"))
            {
                return line.to_owned();
            }
            let mut line = line.to_owned();
            for (name, part) in old.iter() {
                if *name != namespace {
                    line = line.replace(&format!("`{name}`"), &format!("`{namespace}.{part}`"));
                }
            }
            line
        })
        .collect();

    if !declared.is_empty() {
        let parts: Vec<(String, &str)> = declared
            .iter()
            .map(|(name, part)| (local_of(&renames, name).to_owned(), *part))
            .collect();
        while text.ends_with("\n\n") {
            text.pop();
        }
        if !text.ends_with('\n') {
            text.push('\n');
        }
        text.push_str(&export_block(component, &parts));
    }
    text.push_str(stamp);
    Ok(Some(text))
}

/// The local a renamed part took, or its own name.
fn local_of<'a>(renames: &'a [(&str, String)], name: &'a str) -> &'a str {
    renames
        .iter()
        .find(|(old, _)| *old == name)
        .map_or(name, |(_, local)| local.as_str())
}

/// The export list the registry ends a component with, and the comment above
/// it, exactly as the registry writes it — so a reshaped copy and the
/// registry's text agree byte for byte once both are formatted.
pub(super) fn export_block(component: &str, parts: &[(String, &str)]) -> String {
    let namespace = namespace_name(component);
    let file = format!("{component}.js");
    let (first_local, first) = (&parts[0].0, parts[0].1);
    let second = parts.get(1).map_or(first, |(_, part)| *part);
    let mut out = format!(
        "\n/**\n * The parts, under the names `import * as {namespace} from \"./{file}\"` gives them.\n *\n * One name per component (ubugeeei-prod/uf#1453): a page writes `<{namespace}.{first}>`\n * and `<{namespace}.{second}>`, the way it writes `@uniflowed/ui`'s own parts. Each\n * is declared under its full name, so React DevTools and an error say\n * `{first_local}` rather than `{first}`.\n */\nexport {{\n"
    );
    for (local, part) in parts {
        out.push_str(&format!("  {local} as {part},\n"));
    }
    out.push_str("};\n");
    out
}

/// Rename identifiers, never a property after a `.`, a string, a comment or a
/// name in an import: `import { Skeleton as UiSkeleton }` names the package's
/// export, which the copy's own root taking a new name does not change.
fn rename_identifiers(source: &str, renames: &[(&str, String)]) -> String {
    let tokens = uf_flow::scan::tokenize_jsx(source);
    let mut out = String::with_capacity(source.len() + 32);
    let mut at = 0;
    let mut in_import = false;
    for (index, token) in tokens.iter().enumerate() {
        if token.is_ident(source, "import")
            && (uf_flow::scan::starts_statement(&tokens, index) || token.newline_before)
        {
            in_import = true;
            continue;
        }
        if in_import {
            if token.kind == TokenKind::String {
                in_import = false;
            }
            continue;
        }
        if token.kind != TokenKind::Ident {
            continue;
        }
        if index
            .checked_sub(1)
            .is_some_and(|previous| tokens[previous].is_punct(b'.'))
        {
            continue;
        }
        let Some((_, local)) = renames.iter().find(|(old, _)| *old == token.text(source)) else {
            continue;
        };
        out.push_str(&source[at..token.start]);
        out.push_str(local);
        at = token.end;
    }
    out.push_str(&source[at..]);
    out
}

/// `import * as Primitive from "@uniflowed/ui"` and every `Primitive.X`, as
/// the named imports the registry now uses: a namespace or a single component
/// by its name — `Headless` + the name where the file declares that name
/// itself — and anything else, which is a type, as `type X`.
///
/// A copy that imported the package by name already is left as it is.
fn name_headless_imports(source: &str) -> Result<String, String> {
    let tokens = uf_flow::scan::tokenize_jsx(source);
    let Some(star) = tokens.windows(7).find(|it| {
        it[0].is_ident(source, "import")
            && it[1].is_punct(b'*')
            && it[2].is_ident(source, "as")
            && it[4].is_ident(source, "from")
            && it[5].kind == TokenKind::String
            && &it[5].text(source)[1..it[5].text(source).len() - 1] == "@uniflowed/ui"
    }) else {
        return Ok(source.to_owned());
    };
    let local = star[2 + 1].text(source);
    let statement = star[0].start..if star[6].is_punct(b';') {
        star[6].end
    } else {
        star[5].end
    };

    let declared: Vec<&str> = tokens
        .windows(2)
        .filter(|it| {
            matches!(
                it[0].text(source),
                "component" | "function" | "const" | "let" | "type" | "hook" | "class"
            ) && it[1].kind == TokenKind::Ident
        })
        .map(|it| it[1].text(source))
        .collect();
    let alias = |name: &str| {
        if declared.contains(&name) {
            format!("Headless{name}")
        } else {
            name.to_owned()
        }
    };

    let mut values: Vec<String> = Vec::new();
    let mut types: Vec<String> = Vec::new();
    let mut edits: Vec<(std::ops::Range<usize>, String)> = vec![(statement.clone(), String::new())];
    for (index, token) in tokens.iter().enumerate() {
        if !(token.kind == TokenKind::Ident && token.text(source) == local)
            || statement.contains(&token.start)
        {
            continue;
        }
        if index
            .checked_sub(1)
            .is_some_and(|previous| tokens[previous].is_punct(b'.'))
        {
            continue;
        }
        let (Some(dot), Some(name)) = (tokens.get(index + 1), tokens.get(index + 2)) else {
            continue;
        };
        if !dot.is_punct(b'.') || name.kind != TokenKind::Ident {
            return Err(format!(
                "uses `{local}` other than as `{local}.Part`; import what it uses by hand"
            ));
        }
        let name_text = name.text(source);
        let (imported, replacement) = if let Some((namespace, member)) = part(name_text) {
            (
                namespace.to_owned(),
                format!("{}.{member}", alias(namespace)),
            )
        } else if super::ui_namespaces::is_namespace(name_text)
            || HEADLESS_SINGLE.contains(&name_text)
        {
            (name_text.to_owned(), alias(name_text))
        } else if name_text.starts_with(|c: char| c.is_ascii_uppercase()) {
            if !types.iter().any(|it| it == name_text) {
                types.push(name_text.to_owned());
            }
            edits.push((token.start..name.end, name_text.to_owned()));
            continue;
        } else {
            (name_text.to_owned(), name_text.to_owned())
        };
        let spelled = if alias(&imported) == imported {
            imported.clone()
        } else {
            format!("{imported} as {}", alias(&imported))
        };
        if !values.contains(&spelled) {
            values.push(spelled);
        }
        edits.push((token.start..name.end, replacement));
    }
    values.sort();
    types.sort();
    let specifiers: Vec<String> = values
        .into_iter()
        .chain(types.into_iter().map(|name| format!("type {name}")))
        .collect();
    edits[0].1 = format!(
        "import {{ {} }} from \"@uniflowed/ui\";",
        specifiers.join(", ")
    );
    edits.sort_by_key(|(range, _)| range.start);
    let mut out = String::with_capacity(source.len());
    let mut at = 0;
    for (range, text) in edits {
        out.push_str(&source[at..range.start]);
        out.push_str(&text);
        at = range.end;
    }
    out.push_str(&source[at..]);
    Ok(out)
}

/// `context-menu`'s and `menubar`'s hand-on of `menu`'s parts, moved from
/// `export { MenuItem as ContextMenuItem, … }` over a named import of
/// `./menu.js` to `export { Item, … } from "./menu.js"`. Whether it did.
fn rehand_menu_parts(text: &mut String, component: &str, namespace: &str) -> bool {
    if !matches!(component, "context-menu" | "menubar") {
        return false;
    }
    let Some(start) = text.find("\nexport {\n  Menu") else {
        return false;
    };
    let Some(length) = text[start..].find("\n};\n") else {
        return false;
    };
    let block = text[start..start + length + "\n};\n".len()].to_owned();
    let names: Vec<&str> = block
        .lines()
        .filter_map(|line| line.trim().strip_prefix("Menu")?.split(" as ").next())
        .collect();
    if names.is_empty() {
        return false;
    }
    let what = if component == "context-menu" {
        "context menu"
    } else {
        "menubar"
    };
    let mut replacement = format!(
        "\n/**\n * The menu parts, which are `menu.js`'s own: a {what} is a menu opened another\n * way, so `<{namespace}.Item>` is the styled `Menu.Item` and a change to it there\n * reaches both.\n */\nexport {{\n"
    );
    for name in &names {
        replacement.push_str(&format!("  {name},\n"));
    }
    replacement.push_str("} from \"./menu.js\";\n");
    text.replace_range(start..start + block.len(), &replacement);
    // The named import the list was over, which nothing else uses.
    if let Some(open) = text.find("import {\n  MenuCheckboxItem,")
        && let Some(close) = text[open..].find("} from \"./menu.js\";\n")
    {
        let end = open + close + "} from \"./menu.js\";\n".len();
        let end = if text[end..].starts_with('\n') {
            end + 1
        } else {
            end
        };
        text.replace_range(open..end, "");
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::migrate::Plan;
    use crate::commands::migrate::ui_namespaces::{Steps, plan_project};
    use camino::Utf8Path;

    /// The copies a 0.2 project has: each old registry file, as `uf ui add`
    /// wrote it, and each one's text in this registry.
    const OLD_COPIES: &[(&str, &str, &str)] = &[
        (
            "button",
            include_str!("../../../tests/fixtures/migrations/ui-copies/button.js"),
            include_str!("../../../../../registry/ui/button.js"),
        ),
        (
            "calendar",
            include_str!("../../../tests/fixtures/migrations/ui-copies/calendar.js"),
            include_str!("../../../../../registry/ui/calendar.js"),
        ),
        (
            "context-menu",
            include_str!("../../../tests/fixtures/migrations/ui-copies/context-menu.js"),
            include_str!("../../../../../registry/ui/context-menu.js"),
        ),
        (
            "date-picker",
            include_str!("../../../tests/fixtures/migrations/ui-copies/date-picker.js"),
            include_str!("../../../../../registry/ui/date-picker.js"),
        ),
        (
            "dialog",
            include_str!("../../../tests/fixtures/migrations/ui-copies/dialog.js"),
            include_str!("../../../../../registry/ui/dialog.js"),
        ),
        (
            "field",
            include_str!("../../../tests/fixtures/migrations/ui-copies/field.js"),
            include_str!("../../../../../registry/ui/field.js"),
        ),
        (
            "menu",
            include_str!("../../../tests/fixtures/migrations/ui-copies/menu.js"),
            include_str!("../../../../../registry/ui/menu.js"),
        ),
        (
            "range-calendar",
            include_str!("../../../tests/fixtures/migrations/ui-copies/range-calendar.js"),
            include_str!("../../../../../registry/ui/range-calendar.js"),
        ),
        (
            "skeleton",
            include_str!("../../../tests/fixtures/migrations/ui-copies/skeleton.js"),
            include_str!("../../../../../registry/ui/skeleton.js"),
        ),
        (
            "tabs",
            include_str!("../../../tests/fixtures/migrations/ui-copies/tabs.js"),
            include_str!("../../../../../registry/ui/tabs.js"),
        ),
        (
            "toast",
            include_str!("../../../tests/fixtures/migrations/ui-copies/toast.js"),
            include_str!("../../../../../registry/ui/toast.js"),
        ),
        (
            "tooltip",
            include_str!("../../../tests/fixtures/migrations/ui-copies/tooltip.js"),
            include_str!("../../../../../registry/ui/tooltip.js"),
        ),
    ];

    /// A 0.2 project: every old copy, stamped, and a page using them the way
    /// the 0.2 documentation said to.
    fn project() -> tempfile::TempDir {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        std::fs::write(root.join("uf.config.js"), "export default {};\n").unwrap();
        let ui = root.join("app/components/ui");
        std::fs::create_dir_all(&ui).unwrap();
        for (name, old, _) in OLD_COPIES {
            std::fs::write(
                ui.join(format!("{name}.js")),
                uf_ui::stamp::stamped(name, old),
            )
            .unwrap();
        }
        std::fs::write(
            root.join("app/$page.js"),
            "// @flow\nimport * as React from \"@uniflowed/react\";\nimport { DialogRole } from \"@uniflowed/ui\";\n\nimport { Button } from \"./components/ui/button.js\";\nimport {\n  Dialog,\n  DialogContent,\n  DialogTitle,\n  DialogTrigger,\n} from \"./components/ui/dialog.js\";\nimport { Toaster, toast } from \"./components/ui/toast.js\";\n\nexport component Page() {\n  return (\n    <Dialog>\n      <DialogTrigger>Open</DialogTrigger>\n      <DialogContent>\n        <DialogTitle>Title</DialogTitle>\n        <Button onClick={() => toast(\"Saved\")}>Save</Button>\n      </DialogContent>\n      <Toaster />\n    </Dialog>\n  );\n}\n",
        )
        .unwrap();
        temp
    }

    /// What the registry cares about: `uf ui update`, run after the codemod,
    /// takes an unedited copy to this registry's text with no conflict to
    /// resolve. The reshape and the registry's own change of shape are the same
    /// edit, so a three-way merge sees one change twice; everything else the
    /// registry changed since the copy was written merges as the registry's.
    #[test]
    fn a_reshaped_copy_updates_to_this_registrys_text_without_a_conflict() {
        let temp = project();
        let root = Utf8Path::from_path(temp.path()).unwrap();
        let mut plan = Plan::new("codemod");
        plan_project(
            root,
            &mut plan,
            Steps {
                package: true,
                copies: true,
            },
        )
        .unwrap();
        assert!(plan.unmapped.is_empty(), "{:?}", plan.unmapped);

        for (name, _, now) in OLD_COPIES {
            let path = format!("app/components/ui/{name}.js");
            let written = plan
                .changes
                .iter()
                .find(|change| change.path == path)
                .and_then(|change| change.after.clone());
            if *name == "button" {
                // One part, and already importing nothing through a namespace.
                assert!(written.is_none(), "{path} was changed");
                continue;
            }
            let written = written.unwrap_or_else(|| panic!("{path} was not reshaped"));
            // Everything but the stamp, which still names the text it began as.
            let (content, stamp) = written.trim_end().rsplit_once('\n').unwrap();
            assert!(Stamp::parse(stamp).is_some(), "{path} lost its stamp");
            let base = OLD_COPIES
                .iter()
                .find(|(each, _, _)| each == name)
                .map(|(_, old, _)| *old)
                .unwrap();
            let merged = uf_ui::update::merge(base, &format!("{content}\n"), now, "0.2.0");
            assert_eq!(merged.conflicts, 0, "{path}:\n{}", merged.text);
            similar_asserts::assert_eq!(merged.text, *now, "{path}");
        }
    }

    #[test]
    fn a_page_imports_each_copy_as_its_namespace() {
        let temp = project();
        let root = Utf8Path::from_path(temp.path()).unwrap();
        let mut plan = Plan::new("codemod");
        plan_project(
            root,
            &mut plan,
            Steps {
                package: true,
                copies: true,
            },
        )
        .unwrap();
        let page = plan
            .changes
            .iter()
            .find(|change| change.path == "app/$page.js")
            .and_then(|change| change.after.clone())
            .expect("the page is rewritten");
        similar_asserts::assert_eq!(
            page,
            "// @flow\nimport * as React from \"@uniflowed/react\";\nimport { DialogRole } from \"@uniflowed/ui\";\n\nimport { Button } from \"./components/ui/button.js\";\nimport * as Dialog from \"./components/ui/dialog.js\";\nimport { toast } from \"./components/ui/toast.js\";\nimport * as Toast from \"./components/ui/toast.js\";\n\nexport component Page() {\n  return (\n    <Dialog.Root>\n      <Dialog.Trigger>Open</Dialog.Trigger>\n      <Dialog.Content>\n        <Dialog.Title>Title</Dialog.Title>\n        <Button onClick={() => toast(\"Saved\")}>Save</Button>\n      </Dialog.Content>\n      <Toast.Region />\n    </Dialog.Root>\n  );\n}\n"
        );

        // Applied, and run again: nothing is left to do.
        plan.apply(root).unwrap();
        let mut again = Plan::new("codemod");
        plan_project(
            root,
            &mut again,
            Steps {
                package: true,
                copies: true,
            },
        )
        .unwrap();
        assert!(
            again.changes.is_empty(),
            "{:?}",
            again.changes.iter().map(|c| &c.path).collect::<Vec<_>>()
        );
    }

    #[test]
    fn a_relative_specifier_resolves_to_the_copy_it_names() {
        let copies: BTreeMap<String, &'static str> =
            [("app/components/ui/dialog.js".to_owned(), "dialog")].into();
        assert_eq!(
            resolve("app/$page.js", "./components/ui/dialog.js", &copies),
            Some("dialog")
        );
        assert_eq!(
            resolve("app/a/b.js", "../components/ui/dialog", &copies),
            Some("dialog")
        );
        assert_eq!(
            resolve("app/$page.js", "@/components/ui/dialog.js", &copies),
            None
        );
        assert_eq!(resolve("app/$page.js", "./dialog.js", &copies), None);
        assert_eq!(resolve("x.js", "../../dialog.js", &copies), None);
    }
}
