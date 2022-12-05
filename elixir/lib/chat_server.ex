defmodule ChatServer do
  @moduledoc """
  Documentation for `ChatServer`.
  """
  require Logger

  def main([port | _args]) do
    port |> String.to_integer() |> accept()
  end

  @doc """
  Accepts requests on the given `port`.
  """
  def accept(port) do
    {:ok, socket} =
      :gen_tcp.listen(port, [:binary, packet: :line, active: false, reuseaddr: true])

    # For testing
    {:ok, port} = :inet.port(socket)
    IO.puts("localhost:#{port}")

    Logger.info("Accepting connections on port #{port}")
    loop_acceptor(socket)
  end

  defp loop_acceptor(socket) do
    {:ok, client} = :gen_tcp.accept(socket)

    {:ok, pid} =
      DynamicSupervisor.start_child(
        ChatServer.ConnectionSupervisor,
        {ChatServer.Connection, [socket: client]}
      )

    :ok = :gen_tcp.controlling_process(client, pid)
    ChatServer.Connection.socket_ownership_transfered(pid)

    loop_acceptor(socket)
  end
end
