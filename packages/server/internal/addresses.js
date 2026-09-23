// @flow
//
// Internal to `@uniflowed/server`: whether an IP address is one an image
// fetch may connect to.
//
// The endpoint behind `/__uf/image` fetches a URL a stranger chose, from inside
// the network the server runs in. The allow-list decides which *names* it may
// ask for; this decides which *addresses* those names may turn out to be, and
// it is asked about the resolved address — never about the name — because a
// name is whatever its DNS says today. `images.example.com` pointed at
// `169.254.169.254` is a request for the cloud's credential endpoint that
// passed every check made on the string.
//
// # Deny by range, allow what is left of the public unicast space
//
// IPv4: everything in the IANA special-purpose registry that is not globally
// reachable — "this network", private, shared (carrier-grade NAT), loopback,
// link-local, the IETF protocol block, the three documentation ranges, the
// benchmark range, the retired 6to4 relay anycast, multicast, reserved and
// broadcast.
//
// IPv6: only global unicast (`2000::/3`) at all, and within it not the
// documentation, ORCHID, Teredo or other IETF-protocol blocks. Three forms
// carry an IPv4 address inside, and for those the embedded address is the one
// judged — IPv4-mapped (`::ffff:a.b.c.d`), NAT64 (`64:ff9b::/96`) and 6to4
// (`2002::/16`) — because each of them is a way to write `127.0.0.1` that an
// IPv6-only check would call public.
//
// Parsed by hand, in one pass, with no regular expression: the input is a
// resolver's answer, and the rule in `docs/security.md` is that nothing
// untrusted meets a backtracking engine.

/** `[first address, prefix length]`, over the address as a number. */
type Range4 = [number, number];

const BLOCKED_V4: $ReadOnlyArray<Range4> = [
  [ipv4(0, 0, 0, 0), 8], // "this network"
  [ipv4(10, 0, 0, 0), 8], // private
  [ipv4(100, 64, 0, 0), 10], // shared address space (CGNAT)
  [ipv4(127, 0, 0, 0), 8], // loopback
  [ipv4(169, 254, 0, 0), 16], // link-local, and every cloud's metadata service
  [ipv4(172, 16, 0, 0), 12], // private
  [ipv4(192, 0, 0, 0), 24], // IETF protocol assignments
  [ipv4(192, 0, 2, 0), 24], // TEST-NET-1
  [ipv4(192, 88, 99, 0), 24], // 6to4 relay anycast
  [ipv4(192, 168, 0, 0), 16], // private
  [ipv4(198, 18, 0, 0), 15], // benchmarking
  [ipv4(198, 51, 100, 0), 24], // TEST-NET-2
  [ipv4(203, 0, 113, 0), 24], // TEST-NET-3
  [ipv4(224, 0, 0, 0), 4], // multicast
  [ipv4(240, 0, 0, 0), 4], // reserved, and the broadcast address
];

function ipv4(a: number, b: number, c: number, d: number): number {
  return ((a << 24) >>> 0) + (b << 16) + (c << 8) + d;
}

/**
 * Whether `address` is publicly routable, and so one the endpoint may reach.
 *
 * `false` for anything that does not parse, because an address this cannot
 * read is not one it can vouch for.
 */
export function isPublicAddress(address: string): boolean {
  const v4 = parseIpv4(address);
  if (v4 != null) return publicV4(v4);
  const v6 = parseIpv6(address);
  if (v6 != null) return publicV6(v6);
  return false;
}

function publicV4(value: number): boolean {
  return !BLOCKED_V4.some(([first, bits]) => {
    const mask = bits === 0 ? 0 : (0xffffffff << (32 - bits)) >>> 0;
    return (value & mask) >>> 0 === first;
  });
}

/** Eight 16-bit groups. */
function publicV6(groups: $ReadOnlyArray<number>): boolean {
  const embedded = (): number => ((groups[6] << 16) >>> 0) + groups[7];
  const zeroes = (from: number, to: number): boolean =>
    groups.slice(from, to).every((group) => group === 0);

  // IPv4-mapped, ::ffff:a.b.c.d — what a dual-stack socket reports for an
  // IPv4 peer, and the form a resolver hands back for one.
  if (zeroes(0, 5) && groups[5] === 0xffff) return publicV4(embedded());
  // NAT64's well-known prefix, 64:ff9b::/96.
  if (groups[0] === 0x64 && groups[1] === 0xff9b && zeroes(2, 6)) {
    return publicV4(embedded());
  }
  // 6to4, 2002:AABB:CCDD::/48 carries AA.BB.CC.DD.
  if (groups[0] === 0x2002) {
    return publicV4(((groups[1] << 16) >>> 0) + groups[2]);
  }
  // Global unicast is 2000::/3; everything else — ::1, ::, fc00::/7, fe80::/10,
  // multicast, the IPv4-compatible and discard ranges — is not reachable from
  // the public internet by design.
  if ((groups[0] & 0xe000) !== 0x2000) return false;
  // Inside it: 2001::/23 is IETF protocol assignments (Teredo is 2001::/32,
  // ORCHID 2001:10::/28 and 2001:20::/28 are in it too), and 2001:db8::/32 is
  // documentation.
  if (groups[0] === 0x2001 && groups[1] < 0x0200) return false;
  if (groups[0] === 0x2001 && groups[1] === 0x0db8) return false;
  // 3fff::/20, the second documentation prefix.
  if (groups[0] === 0x3fff && groups[1] < 0x1000) return false;
  return true;
}

/** Dotted-quad IPv4, strictly: four decimal octets, no leading zeros. */
function parseIpv4(text: string): number | null {
  const parts = text.split(".");
  if (parts.length !== 4) return null;
  let value = 0;
  for (const part of parts) {
    if (part === "" || part.length > 3 || (part.length > 1 && part.startsWith("0"))) {
      return null;
    }
    let octet = 0;
    for (let index = 0; index < part.length; index += 1) {
      const code = part.charCodeAt(index);
      if (code < 0x30 || code > 0x39) return null;
      octet = octet * 10 + (code - 0x30);
    }
    if (octet > 255) return null;
    value = value * 256 + octet;
  }
  return value;
}

/**
 * IPv6 as eight groups, or `null`.
 *
 * `::` compression, an embedded dotted quad in the last 32 bits, and a zone
 * index (`fe80::1%en0`) — which is dropped, because a zone only ever
 * qualifies a link-local address and a link-local address is refused anyway.
 */
function parseIpv6(input: string): $ReadOnlyArray<number> | null {
  let text = input;
  if (text.startsWith("[") && text.endsWith("]")) text = text.slice(1, -1);
  const zone = text.indexOf("%");
  if (zone !== -1) text = text.slice(0, zone);
  if (!text.includes(":")) return null;

  const halves = text.split("::");
  if (halves.length > 2) return null;
  const head = halves[0] === "" ? [] : halves[0].split(":");
  const tail = halves.length === 2 && halves[1] !== "" ? halves[1].split(":") : [];

  const groups = (parts: $ReadOnlyArray<string>, last: boolean): Array<number> | null => {
    const out: Array<number> = [];
    for (let index = 0; index < parts.length; index += 1) {
      const part = parts[index];
      if (last && index === parts.length - 1 && part.includes(".")) {
        const v4 = parseIpv4(part);
        if (v4 == null) return null;
        out.push(Math.floor(v4 / 0x10000), v4 % 0x10000);
        continue;
      }
      if (part === "" || part.length > 4) return null;
      let group = 0;
      for (let at = 0; at < part.length; at += 1) {
        const digit = hexDigit(part.charCodeAt(at));
        if (digit < 0) return null;
        group = group * 16 + digit;
      }
      out.push(group);
    }
    return out;
  };

  // An embedded IPv4 address is only ever the last thing written.
  const front = groups(head, halves.length === 1);
  const back = groups(tail, true);
  if (front == null || back == null) return null;
  if (halves.length === 1) {
    return front.length === 8 ? front : null;
  }
  const missing = 8 - front.length - back.length;
  if (missing < 1) return null;
  return [...front, ...Array.from({ length: missing }, () => 0), ...back];
}

function hexDigit(code: number): number {
  if (code >= 0x30 && code <= 0x39) return code - 0x30;
  if (code >= 0x61 && code <= 0x66) return code - 0x57;
  if (code >= 0x41 && code <= 0x46) return code - 0x37;
  return -1;
}
