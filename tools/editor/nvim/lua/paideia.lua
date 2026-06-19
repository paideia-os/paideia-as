-- paideia.lua: Neovim configuration for paideia-as.
-- Loaded into ~/.config/nvim/lua/paideia.lua and require'd from init.lua.

local M = {}

function M.setup()
    -- Register the filetype.
    vim.filetype.add({ extension = { pdx = "paideia" } })

    -- Tree-sitter parser registration (assuming nvim-treesitter is installed).
    if pcall(require, "nvim-treesitter.parsers") then
        local parser_config = require("nvim-treesitter.parsers").get_parser_configs()
        parser_config.paideia = {
            install_info = {
                url = "https://github.com/paideia-os/paideia-as",
                files = { "tools/editor/tree-sitter-paideia/src/parser.c" },
                generate_requires_npm = true,
            },
            filetype = "paideia",
        }
    end

    -- LSP setup via nvim-lspconfig.
    if pcall(require, "lspconfig") then
        local lspconfig = require("lspconfig")
        local util = require("lspconfig.util")
        local configs = require("lspconfig.configs")
        if not configs.paideia_lsp then
            configs.paideia_lsp = {
                default_config = {
                    cmd = { "paideia-lsp" },
                    filetypes = { "paideia" },
                    root_dir = util.root_pattern("paideia-os.toml"),
                    settings = {},
                },
            }
        end
        lspconfig.paideia_lsp.setup({})
    end
end

return M
