package discord

import (
	"github.com/approvers/rusty-ponyo/bot"
	"github.com/bwmarrin/discordgo"
	"log"
	"os"
	"os/signal"
	"syscall"
)

type DiscordClient struct {
	services []bot.Bot
	logger   *log.Logger
}

func NewDiscordClient(logger *log.Logger) DiscordClient {
	return DiscordClient{
		services: []bot.Bot{},
		logger:   logger,
	}
}

func (c *DiscordClient) AddBot(bot bot.Bot) {
	c.services = append(c.services, bot)
}

func (c *DiscordClient) Start(token string) error {
	discord, err := discordgo.New("Bot " + token)
	if err != nil {
		return err
	}

	discord.AddHandler(c.onMessage)
	discord.Identify.Intents = discordgo.IntentsGuildMessages

	err = discord.Open()
	if err != nil {
		return err
	}

	c.logger.Println("Discord Bot is now running. Ctrl-C to exit.")

	signalChannel := make(chan os.Signal, 1)
	signal.Notify(signalChannel, syscall.SIGINT, syscall.SIGTERM, os.Interrupt, os.Kill)

	<-signalChannel

	return discord.Close()
}

func (c *DiscordClient) onMessage(s *discordgo.Session, m *discordgo.MessageCreate) {
	msg := DiscordMessage{origin: m}
	ctx := DiscordContext{
		channelID: m.ChannelID,
		session:   s,
	}

	for _, v := range c.services {
		v.OnMessage(&msg, &ctx)
	}
}

type DiscordMessage struct {
	origin *discordgo.MessageCreate
}

func (m *DiscordMessage) Content() string {
	return m.origin.Content
}

func (m *DiscordMessage) IsBot() bool {
	return m.origin.Author.Bot
}

type DiscordContext struct {
	channelID string
	session   *discordgo.Session
}

func (c *DiscordContext) SendMessage(msg string) {
	c.session.ChannelMessageSend(c.channelID, msg)
}
