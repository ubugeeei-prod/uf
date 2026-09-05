-- uf for Neovim: start `uf lsp` for the Flow files of a uf project.
--
-- Configuration, not a plugin. It uses `vim.lsp.start`, which is core Neovim
-- (0.8+), so nothing here depends on nvim-lspconfig — and it reattaches to an
-- already-running client for the same root, which is what `vim.lsp.start`
-- does by default.
--
-- The one thing this file exists to get right is `cmd_cwd`. `uf lsp` reads
-- `uf.config.js` from its working directory, once, at start-up, and that read
-- is where the project's formatter width, quote style and lint levels come
-- from. Without `cmd_cwd` the server inherits Neovim's working directory, so
-- opening a file from `~` would silently format it with uf's defaults instead
-- of the project's.

local M = {}

-- uf's single configuration surface, and therefore the project marker.
local CONFIG = "uf.config.js"

-- The file types `uf_lint`'s `flow/syntax` parses: `.js`, `.jsx`, `.mjs` and
-- `.cjs`, which Neovim calls `javascript` and `javascriptreact`.
local FILETYPES = { "javascript", "javascriptreact" }

--- The uf project a file belongs to, or nil.
--- @param path string
--- @return string|nil
function M.root(path)
  local found = vim.fs.find({ CONFIG }, { path = path, upward = true })[1]
  if not found then
    return nil
  end
  return vim.fs.dirname(found)
end

--- Attach `uf lsp` to a buffer, if it is in a uf project.
--- @param bufnr integer
--- @param opts table
function M.attach(bufnr, opts)
  local name = vim.api.nvim_buf_get_name(bufnr)
  if name == "" then
    return
  end
  local root = M.root(name)
  if not root then
    return
  end

  vim.lsp.start({
    name = "uf",
    cmd = opts.cmd,
    cmd_cwd = root,
    root_dir = root,
  }, { bufnr = bufnr })
end

--- @param opts table|nil
---   cmd            command to run, default { "uf", "lsp" }
---   format_on_save format Flow files with uf on write, default false
function M.setup(opts)
  opts = vim.tbl_extend("force", {
    cmd = { "uf", "lsp" },
    format_on_save = false,
  }, opts or {})

  local group = vim.api.nvim_create_augroup("uf_lsp", { clear = true })

  vim.api.nvim_create_autocmd("FileType", {
    group = group,
    pattern = FILETYPES,
    callback = function(args)
      M.attach(args.buf, opts)
    end,
  })

  if opts.format_on_save then
    vim.api.nvim_create_autocmd("BufWritePre", {
      group = group,
      pattern = { "*.js", "*.jsx", "*.mjs", "*.cjs" },
      callback = function(args)
        -- Only uf: another formatter attached to the same buffer would
        -- disagree with `uf fmt`, and the file is checked by `uf fmt --check`.
        vim.lsp.buf.format({ bufnr = args.buf, name = "uf", timeout_ms = 2000 })
      end,
    })
  end
end

return M
