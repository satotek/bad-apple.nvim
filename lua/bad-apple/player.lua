local Player = {}
Player.__index = Player
local paths = require("bad-apple.paths")

local function read_u16(bytes, offset)
  local a, b = bytes:byte(offset, offset + 1)
  return a + b * 0x100
end

local function read_u32(bytes, offset)
  local a, b, c, d = bytes:byte(offset, offset + 3)
  return a + b * 0x100 + c * 0x10000 + d * 0x1000000
end

function Player.new(options)
  return setmetatable({
    options = options,
    buffer = nil,
    window = nil,
    resize_autocmd = nil,
    process = nil,
    stdin = nil,
    stdout = nil,
    stderr = nil,
    bytes = "",
    paused = false,
    muted = false,
  }, Player)
end

function Player:install_controls()
  local controls = {
    q = function()
      require("bad-apple").stop()
    end,
    ["<Space>"] = function()
      require("bad-apple").toggle_pause()
    end,
    m = function()
      self:toggle_mute()
    end,
    h = function()
      self:seek(-5)
    end,
    l = function()
      self:seek(5)
    end,
    r = function()
      self:send("restart")
    end,
  }
  for lhs, callback in pairs(controls) do
    vim.keymap.set("n", lhs, callback, { buffer = self.buffer, silent = true })
  end
end

function Player:create_buffer()
  local buffer = vim.api.nvim_create_buf(false, true)
  self.buffer = buffer

  vim.bo[buffer].buftype = "nofile"
  vim.bo[buffer].bufhidden = "wipe"
  vim.bo[buffer].swapfile = false
  vim.bo[buffer].filetype = "bad-apple"
  vim.bo[buffer].modifiable = false
  vim.api.nvim_buf_set_name(buffer, "bad-apple://player")
  vim.api.nvim_win_set_buf(0, buffer)
  self.window = vim.api.nvim_get_current_win()

  vim.wo.wrap = false
  vim.wo.number = false
  vim.wo.relativenumber = false
  vim.wo.signcolumn = "no"
  vim.wo.cursorline = false

  self:install_controls()

  self.resize_autocmd = vim.api.nvim_create_autocmd({ "VimResized", "WinResized" }, {
    callback = function()
      vim.schedule(function()
        self:resize()
      end)
    end,
  })

  vim.api.nvim_create_autocmd("BufWipeout", {
    buffer = buffer,
    once = true,
    callback = function()
      self:stop(false)
    end,
  })
end

function Player:apply_patch(payload)
  local message_type = payload:byte(1)
  if message_type ~= 1 then
    return
  end
  local row_count = read_u16(payload, 6)
  local cursor = 8
  local patches = {}
  for _ = 1, row_count do
    local row = read_u16(payload, cursor)
    local length = read_u32(payload, cursor + 2)
    cursor = cursor + 6
    patches[#patches + 1] = { row = row, text = payload:sub(cursor, cursor + length - 1) }
    cursor = cursor + length
  end

  vim.schedule(function()
    if not self.buffer or not vim.api.nvim_buf_is_valid(self.buffer) then
      return
    end
    vim.bo[self.buffer].modifiable = true
    for _, patch in ipairs(patches) do
      local line_count = vim.api.nvim_buf_line_count(self.buffer)
      if patch.row >= line_count then
        vim.api.nvim_buf_set_lines(
          self.buffer,
          line_count,
          -1,
          false,
          vim.fn["repeat"]({ "" }, patch.row - line_count + 1)
        )
      end
      vim.api.nvim_buf_set_lines(self.buffer, patch.row, patch.row + 1, false, { patch.text })
    end
    vim.bo[self.buffer].modifiable = false
  end)
end

function Player:consume(data)
  self.bytes = self.bytes .. data
  while #self.bytes >= 4 do
    local payload_size = read_u32(self.bytes, 1)
    if #self.bytes < payload_size + 4 then
      return
    end
    local payload = self.bytes:sub(5, payload_size + 4)
    self.bytes = self.bytes:sub(payload_size + 5)
    self:apply_patch(payload)
  end
end

function Player:start(source)
  local engine = paths.resolve_engine(self.options.engine_path)
  if not engine then
    error("bav-engine was not found; run cargo build or configure engine_path")
  end
  local movie = paths.resolve_movie(source or self.options.movie_path)
  if not movie then
    error("movie.bav was not found; run :BadApple or configure movie_path")
  end

  self:create_buffer()
  local columns = math.max(vim.api.nvim_win_get_width(0), 20)
  local rows = math.max(vim.api.nvim_win_get_height(0) - 1, 8)
  local arguments = { movie, tostring(columns), tostring(rows) }
  local audio = paths.resolve_audio(self.options.audio_path)
  if audio then
    arguments[#arguments + 1] = audio
  end
  self.stdout = vim.uv.new_pipe(false)
  self.stderr = vim.uv.new_pipe(false)
  self.stdin = vim.uv.new_pipe(false)
  self.process = vim.uv.spawn(engine, {
    args = arguments,
    stdio = { self.stdin, self.stdout, self.stderr },
  }, function(code)
    if code ~= 0 then
      vim.schedule(function()
        vim.notify("bad-apple.nvim: bav-engine exited with code " .. code, vim.log.levels.ERROR)
      end)
    end
  end)
  if not self.process then
    self.stdout:close()
    self.stderr:close()
    error("failed to start bav-engine")
  end

  self.stdout:read_start(function(error_message, data)
    if error_message then
      vim.schedule(function()
        vim.notify("bad-apple.nvim: " .. error_message, vim.log.levels.ERROR)
      end)
    elseif data then
      self:consume(data)
    end
  end)
  self.stderr:read_start(function(_, data)
    if data and data ~= "" then
      vim.schedule(function()
        vim.notify("bad-apple.nvim: " .. vim.trim(data), vim.log.levels.ERROR)
      end)
    end
  end)
end

function Player:toggle_mute()
  if self:send("m") then
    self.muted = not self.muted
    vim.notify(self.muted and "bad-apple.nvim: Muted" or "bad-apple.nvim: Unmuted")
  end
end

function Player:toggle_pause()
  if self:send("p") then
    self.paused = not self.paused
  end
end

function Player:seek(seconds)
  self:send("seek " .. seconds)
end

function Player:send(command)
  if self.stdin and not self.stdin:is_closing() then
    self.stdin:write(command .. "\n")
    return true
  end
  return false
end

function Player:resize()
  if not self.window or not vim.api.nvim_win_is_valid(self.window) then
    return
  end
  if vim.api.nvim_win_get_buf(self.window) ~= self.buffer then
    return
  end
  local columns = math.max(vim.api.nvim_win_get_width(self.window), 20)
  local rows = math.max(vim.api.nvim_win_get_height(self.window) - 1, 8)
  self:send(string.format("resize %d %d", columns, rows))
end

function Player:stop(close_buffer)
  for _, pipe in ipairs({ self.stdin, self.stdout, self.stderr }) do
    if pipe and not pipe:is_closing() then
      pipe:read_stop()
      pipe:close()
    end
  end
  self.stdout = nil
  self.stderr = nil
  self.stdin = nil

  if self.process and not self.process:is_closing() then
    self.process:kill("sigterm")
    self.process:close()
  end
  self.process = nil

  if self.resize_autocmd then
    pcall(vim.api.nvim_del_autocmd, self.resize_autocmd)
    self.resize_autocmd = nil
  end

  if close_buffer ~= false and self.buffer and vim.api.nvim_buf_is_valid(self.buffer) then
    vim.api.nvim_buf_delete(self.buffer, { force = true })
  end
end

return Player
