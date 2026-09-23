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
--
-- The second thing is the TypeScript server most configurations also start
-- for `javascript` buffers (`ts_ls`, `vtsls`, `denols`). It reads a Flow file
-- as TypeScript and reports `component`, `hook`, `match` and every annotation
-- as errors beside uf's. `exclude` detaches those clients from buffers in a uf
-- project — only there, so the same Neovim still gets them in a TypeScript
-- project. It works on `LspAttach`, whatever started the client
-- (nvim-lspconfig, `vim.lsp.enable`, a plugin), because a client is always
-- attached by name.

local M = {}

-- uf's single configuration surface, and therefore the project marker.
local CONFIG = "uf.config.js"

-- The file types `uf_lint`'s `flow/syntax` parses: `.js`, `.jsx`, `.mjs` and
-- `.cjs`, which Neovim calls `javascript` and `javascriptreact`.
local FILETYPES = { "javascript", "javascriptreact" }

-- The servers nvim-lspconfig names for JavaScript that are not Flow.
M.TYPESCRIPT_SERVERS = { "ts_ls", "tsserver", "vtsls", "denols", "typescript-tools" }

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

--- The uf project a buffer's file belongs to, or nil.
--- @param bufnr integer
--- @return string|nil
function M.buffer_root(bufnr)
  local name = vim.api.nvim_buf_get_name(bufnr)
  if name == "" then
    return nil
  end
  return M.root(vim.fs.dirname(name))
end

--- Attach `uf lsp` to a buffer, if it is in a uf project.
--- @param bufnr integer
--- @param opts table
function M.attach(bufnr, opts)
  local root = M.buffer_root(bufnr)
  if not root then
    return
  end

  -- In the buffer's own context: Neovim 0.8's `vim.lsp.start` attaches to
  -- the current buffer whatever `bufnr` says, and `:UfRestart` attaches
  -- buffers that are not current.
  vim.api.nvim_buf_call(bufnr, function()
    vim.lsp.start({
      name = "uf",
      cmd = opts.cmd,
      cmd_cwd = root,
      root_dir = root,
    }, { bufnr = bufnr })
  end)
end

--- Every running client with this name. `vim.lsp.get_clients` is 0.10+;
--- `get_active_clients` is the 0.8 and 0.9 spelling.
--- @param name string
--- @return table
local function clients_named(name)
  if vim.lsp.get_clients then
    return vim.lsp.get_clients({ name = name })
  end
  return vim.tbl_filter(function(client)
    return client.name == name
  end, vim.lsp.get_active_clients())
end

--- Stop every uf client and start them again for the loaded buffers.
--- The server reads `uf.config.js` once, at start-up; this is how an edit to
--- it takes effect.
--- @param opts table
function M.restart(opts)
  for _, client in ipairs(clients_named("uf")) do
    -- `client:stop()` from 0.11, where `vim.lsp.stop_client` is deprecated.
    if vim.fn.has("nvim-0.11") == 1 then
      client:stop()
    else
      vim.lsp.stop_client(client.id)
    end
  end
  -- A stopped client takes a moment to go; `vim.lsp.start` would reuse it
  -- if asked before it has.
  vim.defer_fn(function()
    for _, bufnr in ipairs(vim.api.nvim_list_bufs()) do
      if vim.api.nvim_buf_is_loaded(bufnr) and vim.tbl_contains(FILETYPES, vim.bo[bufnr].filetype) then
        M.attach(bufnr, opts)
      end
    end
  end, 500)
end

--- Drop what a TypeScript client publishes about files in a uf project.
---
--- Detaching a client from a buffer does not stop it publishing diagnostics
--- for that file — the `didOpen` it was already sent is answered after the
--- detach — and Neovim shows published diagnostics whether or not the client
--- is attached. So its diagnostics handler is wrapped, once per client, to
--- publish an empty list for a file in a uf project instead.
--- @param client table
local function keep_quiet(client)
  if client.uf_quiet then
    return
  end
  client.uf_quiet = true
  local method = "textDocument/publishDiagnostics"
  local inner = client.handlers[method] or vim.lsp.handlers[method]
  client.handlers[method] = function(err, result, ctx, config)
    if result ~= nil and result.uri ~= nil and M.root(vim.fs.dirname(vim.uri_to_fname(result.uri))) ~= nil then
      result = vim.tbl_extend("force", result, { diagnostics = {} })
    end
    return inner(err, result, ctx, config)
  end
end

--- @param opts table|nil
---   cmd            command to run, default { "uf", "lsp" }
---   format_on_save format Flow files with uf on write, default false
---   exclude        client names to detach from buffers in a uf project,
---                  default M.TYPESCRIPT_SERVERS; {} to keep them
function M.setup(opts)
  opts = vim.tbl_extend("force", {
    cmd = { "uf", "lsp" },
    format_on_save = false,
    exclude = M.TYPESCRIPT_SERVERS,
  }, opts or {})

  local group = vim.api.nvim_create_augroup("uf_lsp", { clear = true })

  vim.api.nvim_create_autocmd("FileType", {
    group = group,
    pattern = FILETYPES,
    callback = function(args)
      M.attach(args.buf, opts)
    end,
  })

  if #opts.exclude > 0 then
    vim.api.nvim_create_autocmd("LspAttach", {
      group = group,
      callback = function(args)
        local client = vim.lsp.get_client_by_id(args.data.client_id)
        if client == nil or not vim.tbl_contains(opts.exclude, client.name) then
          return
        end
        if not vim.tbl_contains(FILETYPES, vim.bo[args.buf].filetype) then
          return
        end
        if M.buffer_root(args.buf) == nil then
          return
        end
        keep_quiet(client)
        -- Scheduled: detaching from inside the attach that is running is
        -- not allowed.
        vim.schedule(function()
          if vim.api.nvim_buf_is_valid(args.buf) then
            vim.lsp.buf_detach_client(args.buf, client.id)
            vim.diagnostic.reset(vim.lsp.diagnostic.get_namespace(client.id), args.buf)
          end
        end)
      end,
    })
  end

  if opts.format_on_save then
    vim.api.nvim_create_autocmd("BufWritePre", {
      group = group,
      pattern = { "*.js", "*.jsx", "*.mjs", "*.cjs" },
      callback = function(args)
        if M.buffer_root(args.buf) == nil then
          return
        end
        -- Only uf: another formatter attached to the same buffer would
        -- disagree with `uf fmt`, and the file is checked by `uf fmt --check`.
        vim.lsp.buf.format({ bufnr = args.buf, name = "uf", timeout_ms = 2000 })
      end,
    })
  end

  vim.api.nvim_create_user_command("UfRestart", function()
    M.restart(opts)
  end, { desc = "Restart uf lsp, re-reading uf.config.js" })
end

return M
