// @flow
import { stylex } from "@uniflowed/stylex/native";
export const styles = stylex.create({
  page: { flex: 1, backgroundColor: "#f6f5ed" },
  content: { padding: 24, gap: 20 },
  eyebrow: { fontSize: 11, fontWeight: "700", letterSpacing: 2, color: "#647760" },
  title: { fontSize: 32, fontWeight: "700", letterSpacing: -1, color: "#27382c" },
  subtitle: { fontSize: 14, lineHeight: 21, color: "#74786d" },
  card: {
    padding: 20,
    gap: 12,
    backgroundColor: "#fffdf8",
    borderColor: "#e1dfd5",
    borderWidth: 1,
    borderRadius: 16,
  },
  author: { fontSize: 15, fontWeight: "600", color: "#27382c" },
  body: { fontSize: 16, lineHeight: 25, color: "#3e473e" },
  link: { fontSize: 14, fontWeight: "600", color: "#315744" },
});
