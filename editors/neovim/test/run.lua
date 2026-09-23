-- Headless checks for lua/uf.lua, in a real Neovim:
--
--   nvim --headless -n -u NONE -i NONE -c "luafile editors/neovim/test/run.lua"
--
-- `editors/test/fake-lsp.cjs` stands in for both `uf lsp` and a TypeScript
-- server; each answers `initialize` and publishes one diagnostic per opened
-- file whose message is its own working directory. So the checks are about
-- what `uf.lua` decides — whether uf attaches, in which directory it is
-- started, and whether a TypeScript client is kept off a uf project — not
-- about uf's answers, which `tests/library/lsp.test.js` checks against the
-- real server. Exits non-zero on the first failure.

local here = vim.fs.dirname(vim.fn.fnamemodify(debug.getinfo(1, "S").source:sub(2), ":p"))
local editors = vim.fs.dirname(vim.fs.dirname(here))
local fake = editors .. "/test/fake-lsp.cjs"
vim.opt.runtimepath:prepend(editors .. "/neovim")
package.loaded.uf = nil
local uf = require("uf")

local failures = 0
local function check(what, ok, detail)
  if ok then
    io.stdout:write("ok   " .. what .. "\n")
  else
    failures = failures + 1
    io.stdout:write("FAIL " .. what .. (detail and (": " .. vim.inspect(detail)) or "") .. "\n")
  end
end

local function project(with_config)
  local dir = vim.fn.resolve(vim.fn.tempname())
  vim.fn.mkdir(dir .. "/app", "p")
  if with_config then
    vim.fn.writefile({ "export default {};" }, dir .. "/uf.config.js")
  end
  vim.fn.writefile({ "// @flow", "export component Button() {}" }, dir .. "/app/button.js")
  return dir
end

local function attached(bufnr)
  local names = {}
  local get = vim.lsp.get_clients or vim.lsp.get_active_clients
  for _, client in ipairs(get({ bufnr = bufnr })) do
    names[client.name] = true
  end
  return names
end

local function messages(bufnr)
  local out = {}
  for _, d in ipairs(vim.diagnostic.get(bufnr)) do
    out[#out + 1] = d.source .. " " .. d.message
  end
  table.sort(out)
  return out
end

local function main()
-- `-u NONE` leaves file type detection off, and every decision here hangs off `FileType`.
vim.cmd("filetype on")
uf.setup({ cmd = { "node", fake, "uf" } })

-- What a user's own configuration does: a TypeScript server for every
-- JavaScript buffer, started in whatever directory Neovim is in.
vim.api.nvim_create_autocmd("FileType", {
  pattern = { "javascript", "javascriptreact" },
  callback = function(args)
    vim.lsp.start({ name = "ts_ls", cmd = { "node", fake, "ts_ls" }, root_dir = vim.fn.getcwd() }, { bufnr = args.buf })
  end,
})

-- A uf project: uf attaches, started in the project; ts_ls does not stay.
local root = project(true)
vim.cmd("edit " .. root .. "/app/button.js")
local buf = vim.api.nvim_get_current_buf()
vim.wait(5000, function()
  local names = attached(buf)
  return names.uf and not names.ts_ls and #messages(buf) == 1
end, 50)
local names = attached(buf)
check("uf attaches in a uf project", names.uf == true, names)
check("ts_ls is detached in a uf project", names.ts_ls == nil, names)
check("only uf's diagnostics remain, and it runs in the project", vim.deep_equal(messages(buf), { "uf cwd=" .. root }), messages(buf))

-- A project without uf.config.js: no uf, ts_ls stays.
local plain = project(false)
vim.cmd("edit " .. plain .. "/app/button.js")
local other = vim.api.nvim_get_current_buf()
vim.wait(5000, function()
  return attached(other).ts_ls == true and #messages(other) == 1
end, 50)
names = attached(other)
check("uf does not attach outside a uf project", names.uf == nil, names)
check("ts_ls stays outside a uf project", names.ts_ls == true, names)

-- :UfRestart starts a fresh uf client for the uf buffer.
local before = {}
local get = vim.lsp.get_clients or vim.lsp.get_active_clients
for _, client in ipairs(get({ name = "uf" })) do
  before[client.id] = true
end
vim.cmd("UfRestart")
local restarted = false
vim.wait(5000, function()
  for _, client in ipairs(get({ bufnr = buf })) do
    if client.name == "uf" and not before[client.id] then
      restarted = true
    end
  end
  return restarted
end, 50)
check(":UfRestart starts a new uf client", restarted)
end

local ok, err = pcall(main)
if not ok then
  failures = failures + 1
  io.stdout:write("FAIL " .. tostring(err) .. "\n")
end

if failures > 0 then
  vim.cmd("cquit " .. failures)
else
  vim.cmd("qall!")
end
