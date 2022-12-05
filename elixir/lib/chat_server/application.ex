defmodule ChatServer.Application do
  use Application

  @impl true
  def start(_type, _args) do
    # port = String.to_integer(System.get_env("PORT") || "8000")

    children = [
      # Supervisor.child_spec({Task, fn -> ChatServer.accept(port) end}, restart: :permanent),
      {DynamicSupervisor, name: ChatServer.ConnectionSupervisor, strategy: :one_for_one},
      {ChatServer.Logins, name: ChatServer.Logins},
      {ChatServer.Channels, name: ChatServer.Channels},
      {Registry, name: ChatServer.ChannelRegistry, keys: :duplicate}
    ]

    opts = [strategy: :one_for_one, name: ChatServer.Supervisor]
    Supervisor.start_link(children, opts)
  end
end
