package bot

type Message interface {
	Content() string
	IsBot() bool
}

type Context interface {
	SendMessage(string)
}

type Bot interface {
	OnMessage(Message, Context)
}
