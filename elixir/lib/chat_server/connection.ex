defmodule ChatServer.Connection do
  @moduledoc """
  Manage a socket connection to a server or client.
  """
  use GenServer, restart: :temporary
  require Logger

  defstruct [
    :socket,
    # :unknown, :client, or :server
    type: :client,
    name: nil
  ]

  def start_link(opts) do
    socket = Keyword.fetch!(opts, :socket)
    GenServer.start_link(__MODULE__, socket, opts)
  end

  @doc """
  Call this function after calling `:gen_tcp.controlling_process/2`.

  This exists because at the time of calling `Genserver.init/1` the process
  does not own the socket and cannot because `:gen_tcp.controlling_process/2` takes the pid
  returned by `Genserver.init/1`. Currently this is used to change the socket to active mode.
  """
  def socket_ownership_transfered(pid) do
    GenServer.cast(pid, :socket_owner)
  end

  def send(pid, message) do
    GenServer.cast(pid, {:send, message})
  end

  ## Callbacks

  def init(socket) do
    {:ok, %__MODULE__{socket: socket}}
  end

  def handle_cast(:socket_owner, state) do
    :inet.setopts(state.socket, active: :once)
    {:noreply, state}
  end

  def handle_cast({:send, message}, state) do
    :gen_tcp.send(state.socket, message)
    {:noreply, state}
  end

  def handle_info({:tcp, _socket, data}, state) do
    state =
      case parse(data) do
        {:ok, cmd} ->
          execute(state, cmd)

        {:error, :unrecognized} ->
          Logger.warn("#{inspect(self())} received strange tcp traffic: #{data}")
          state
      end

    :inet.setopts(state.socket, active: :once)
    {:noreply, state}
  end

  def handle_info({:tcp_closed, _socket}, socket) do
    {:stop, :normal, socket}
  end

  def handle_info({:tcp_error, _socket, reason}, socket) do
    {:stop, reason, socket}
  end

  def handle_info(message, socket) do
    Logger.info("#{inspect(self())} received message #{inspect(message)}")
    {:noreply, socket}
  end

  ## Private functions

  # @Todo this is slow because of the join (and split) for SAY, the parse is O(n) when we could have O(1)
  defp parse(line) do
    case String.split(line) do
      ["REGISTER", username, password] -> {:ok, {:register, username, password}}
      ["LOGIN", username, password] -> {:ok, {:login, username, password}}
      ["CREATE", channel] -> {:ok, {:create, channel}}
      ["JOIN", channel] -> {:ok, {:join, channel}}
      ["CHANNELS"] -> {:ok, :channels}
      ["SAY", channel | rest] when rest != [] -> {:ok, {:say, channel, Enum.join(rest, " ")}}
      _ -> {:error, :unrecognized}
    end
  end

  defp execute(state, cmd) do
    case cmd do
      {:register, username, password} -> register(state, username, password)
      {:login, username, password} -> login(state, username, password)
      {:create, channel} -> create(state, channel)
      {:join, channel} -> join(state, channel)
      :channels -> list_channels(state)
      {:say, channel, message} -> say(state, channel, message)
    end
  end

  defp register(state, username, password) do
    result =
      case ChatServer.Logins.add(username, password) do
        :ok -> 1
        :error -> 0
      end

    :gen_tcp.send(state.socket, "RESULT REGISTER #{result}\n")
    state
  end

  defp login(state, username, password) do
    {result, state} =
      case ChatServer.Logins.lookup(username, password) do
        :ok ->
          {1, %{state | name: username}}

        :error ->
          {0, state}
      end

    :gen_tcp.send(state.socket, "RESULT LOGIN #{result}\n")
    state
  end

  defp create(state, channel) do
    result =
      case ChatServer.Channels.create(channel) do
        :ok ->
          1

        :error ->
          0
      end

    :gen_tcp.send(state.socket, "RESULT CREATE #{channel} #{result}\n")
    state
  end

  defp list_channels(state) do
    channels = ChatServer.Channels.channels()

    message =
      if MapSet.size(channels) == 0 do
        "RESULT CHANNELS\n"
      else
        ["RESULT CHANNELS ", Enum.join(channels, ", "), "\n"]
      end

    :gen_tcp.send(state.socket, message)
    state
  end

  defp join(state, channel) do
    result =
      if ChatServer.Channels.exists?(channel) and client_logged_in?(state) and
           not client_channel_member?(channel) do
        Registry.register(ChatServer.ChannelRegistry, channel, state.name)
        1
      else
        0
      end

    :gen_tcp.send(state.socket, "RESULT JOIN #{channel} #{result}\n")
    state
  end

  defp say(state, channel, message) do
    result =
      if ChatServer.Channels.exists?(channel) and client_logged_in?(state) and
           client_channel_member?(channel) do
        message = "RECV #{state.name} #{channel} #{message}\n"

        # @Todo Tasks?
        Registry.dispatch(ChatServer.ChannelRegistry, channel, fn entries ->
          for {pid, _username} <- entries do
            ChatServer.Connection.send(pid, message)
          end
        end)

        1
      else
        0
      end

    :gen_tcp.send(state.socket, "RESULT SAY #{channel} #{result}\n")
    state
  end

  defp client_logged_in?(state) do
    state.type == :client and state.name != nil
  end

  defp client_channel_member?(channel) do
    Registry.lookup(ChatServer.ChannelRegistry, channel)
    |> Enum.any?(fn
      {pid, _name} when pid == self() -> true
      _ -> false
    end)
  end
end
