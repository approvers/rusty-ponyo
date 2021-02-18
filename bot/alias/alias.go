package alias

import (
	"fmt"
	"github.com/approvers/rusty-ponyo/bot"
)

type messageAliasBot struct{}

func NewMessageAliasBot() *messageAliasBot {
	return &messageAliasBot{}
}

func (b *messageAliasBot) OnMessage(msg bot.Message, ctx bot.Context) {
	if msg.IsBot() {
		return
	}

	content := msg.Content()
	parsed := parse(content)

	switch parsed.kind {
	case noMatch:
	case parseError:
		ctx.SendMessage(parsed.errorMsg)
	}

	fmt.Printf("%#v\n", parsed)
}
