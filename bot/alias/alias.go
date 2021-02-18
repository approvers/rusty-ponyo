package alias

import (
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
	case dataAvailable:
		ctx.SendMessage(onCommand(parsed.data))
	}
}

func help() string {
	const helpText string = "```asciidoc\n" +
		"= rusty_ponyo::alias =\n" +
		"g!alias [subcommand] [args...]\n" +
		">>> 引数において \" は省略できません <<<\n" +
		"= subcommands =\n" +
		"    help                         :: この文を出します\n" +
		"    make \"[キー]\" \"[メッセージ]\" :: エイリアスを作成します\n" +
		"    delete \"[キー]\"              :: エイリアスを削除します\n" +
		"```"

	return helpText
}

func onCommand(data parseData) string {
	if data.subCommand == "" {
		return help()
	}

	switch data.subCommand {
	case "make":
		return "called make"
	case "delete":
		return "called delete"
	case "help":
		fallthrough
	default:
		return help()
	}
}
