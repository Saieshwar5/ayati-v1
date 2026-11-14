use sea_orm_migration::prelude::*;
pub(crate) struct Communications;
impl MigrationName for Communications {
    fn name(&self) -> &str {
        "m20261026_000032_native_communications"
    }
}
#[async_trait::async_trait]
impl MigrationTrait for Communications {
    async fn up(&self, m: &SchemaManager) -> Result<(), DbErr> {
        m.get_connection()
            .execute_unprepared(include_str!("communications.sql"))
            .await?;
        Ok(())
    }
    async fn down(&self, _: &SchemaManager) -> Result<(), DbErr> {
        Err(DbErr::Custom(
            "Communication receipts require a forward migration".into(),
        ))
    }
}

/// Schema 111: WhatsApp numbers Cuckoo provides and people's verified mobiles.
/// Kept for the schema's history; schema 114 removes them.
pub(crate) struct WhatsappNumbers;
impl MigrationName for WhatsappNumbers {
    fn name(&self) -> &str {
        "m20261110_000111_whatsapp_numbers"
    }
}
#[async_trait::async_trait]
impl MigrationTrait for WhatsappNumbers {
    async fn up(&self, m: &SchemaManager) -> Result<(), DbErr> {
        m.get_connection()
            .execute_unprepared(include_str!("whatsapp.sql"))
            .await?;
        Ok(())
    }
    async fn down(&self, _: &SchemaManager) -> Result<(), DbErr> {
        Err(DbErr::Custom(
            "WhatsApp numbers require a forward migration".into(),
        ))
    }
}

/// Schema 114: WhatsApp removed. Its tables go and live WhatsApp endpoints
/// are switched off.
pub(crate) struct WhatsappRemoved;
impl MigrationName for WhatsappRemoved {
    fn name(&self) -> &str {
        "m20261114_000114_whatsapp_removed"
    }
}
#[async_trait::async_trait]
impl MigrationTrait for WhatsappRemoved {
    async fn up(&self, m: &SchemaManager) -> Result<(), DbErr> {
        m.get_connection()
            .execute_unprepared(include_str!("whatsapp_removed.sql"))
            .await?;
        Ok(())
    }
    async fn down(&self, _: &SchemaManager) -> Result<(), DbErr> {
        Err(DbErr::Custom(
            "Removing WhatsApp requires a forward migration".into(),
        ))
    }
}

/// Schema 54: real transports, delivery reasons and read cursors.
pub(crate) struct CommunicationGateway;
impl MigrationName for CommunicationGateway {
    fn name(&self) -> &str {
        "m20261101_000054_communication_gateway"
    }
}
#[async_trait::async_trait]
impl MigrationTrait for CommunicationGateway {
    async fn up(&self, m: &SchemaManager) -> Result<(), DbErr> {
        m.get_connection()
            .execute_unprepared(include_str!("gateway.sql"))
            .await?;
        Ok(())
    }
    async fn down(&self, _: &SchemaManager) -> Result<(), DbErr> {
        Err(DbErr::Custom(
            "The communication gateway requires a forward migration".into(),
        ))
    }
}

/// Schema 112: doors with proven identity share the person's direct room.
pub(crate) struct DoorRooms;
impl MigrationName for DoorRooms {
    fn name(&self) -> &str {
        "m20261114_000112_door_rooms"
    }
}
#[async_trait::async_trait]
impl MigrationTrait for DoorRooms {
    async fn up(&self, m: &SchemaManager) -> Result<(), DbErr> {
        m.get_connection()
            .execute_unprepared(include_str!("door_rooms.sql"))
            .await?;
        Ok(())
    }
    async fn down(&self, _: &SchemaManager) -> Result<(), DbErr> {
        Err(DbErr::Custom(
            "Door rooms require a forward migration".into(),
        ))
    }
}

/// Schema 113: agents' email addresses and the mail threads they answer in.
pub(crate) struct AgentEmail;
impl MigrationName for AgentEmail {
    fn name(&self) -> &str {
        "m20261114_000113_agent_email"
    }
}
#[async_trait::async_trait]
impl MigrationTrait for AgentEmail {
    async fn up(&self, m: &SchemaManager) -> Result<(), DbErr> {
        m.get_connection()
            .execute_unprepared(include_str!("email.sql"))
            .await?;
        Ok(())
    }
    async fn down(&self, _: &SchemaManager) -> Result<(), DbErr> {
        Err(DbErr::Custom(
            "Agent email requires a forward migration".into(),
        ))
    }
}
