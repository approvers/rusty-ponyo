package mongo

import (
	"context"
	"time"

	"github.com/approvers/rusty-ponyo/model"
	"go.mongodb.org/mongo-driver/bson"
	"go.mongodb.org/mongo-driver/mongo"
	"go.mongodb.org/mongo-driver/mongo/options"
)

type MongoDB struct {
	mongo *mongo.Client
}

func NewMongoDB(uri string) (MongoDB, error) {
	client, err := mongo.Connect(context.Background(), options.Client().ApplyURI(uri))

	if err != nil {
		return MongoDB{}, err
	}

	return MongoDB{mongo: client}, nil
}

func (m *MongoDB) CloseMongoDB(ctx context.Context) error {
	return m.mongo.Disconnect(ctx)
}

type mongoMessageAlias struct {
	key              string    `bson:"key,omitempty"`
	message          string    `bson:"message,omitempty"`
	creatorDiscordID string    `bson:"creatorDiscordID,omitempty"`
	createdAt        time.Time `bson:"createdAt,omitempty"`
}

const (
	messageAliasDatabaseName   = "ponyo"
	messageAliasCollectionName = "aliases"
)

func (m *MongoDB) SaveMessageAlias(alias model.MessageAlias) {
	doc := mongoMessageAlias{
		key:              alias.Key,
		message:          alias.Message,
		creatorDiscordID: alias.CreatorDiscordID,
		createdAt:        alias.CreatedAt,
	}

	m.mongo.
		Database(messageAliasDatabaseName).
		Collection(messageAliasCollectionName).
		InsertOne(context.Background(), doc)
}

func (m *MongoDB) GetMessageAlias(key string) model.MessageAlias {
	fetchResult := m.mongo.
		Database(messageAliasDatabaseName).
		Collection(messageAliasCollectionName).
		FindOne(context.Background(), bson.D{{"key", key}})

	var result model.MessageAlias
	fetchResult.Decode(&result)
}

func (m *MongoDB) DeleteMessageAlias(key string) bool {}

func (m *MongoDB) ListMessageAlias(offset, limit int) []model.MessageAlias {}
