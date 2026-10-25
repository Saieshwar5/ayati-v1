<Frame name="Connections and delegated setup" w={1200} h={820} flex="row" p={12} gap={12} bg="#F1F2F4">
<Frame w={64} h="fill" flex="col" gap={28} p={12}><Text size={20} weight="bold" color="#252A31">C</Text><Text size={12}>Home</Text><Text size={12}>Apps</Text><Text size={12}>Chat</Text><Text size={12}>Agents</Text><Text size={12} weight="bold">Settings</Text></Frame>
<Frame w="fill" h="fill" flex="col" gap={22} p={28} rounded={18} bg="#FFFFFF">
<Text size={26} weight="bold" color="#252A31">Settings</Text>
<Frame w="fill" h="hug" flex="row" gap={24}><Text size={14} color="#6B7079">Personal</Text><Text size={14} weight="bold">Workspace</Text><Text size={14} color="#6B7079">Acme workspace</Text></Frame>
<Frame w="fill" h="hug" flex="row" gap={24}><Text size={14}>General</Text><Text size={14}>People</Text><Text size={14}>Access</Text><Text size={14} weight="bold">Connections</Text><Text size={14}>Activity</Text></Frame>
<Rectangle w="fill" h={1} fill="#E5E7EA" />
<Text size={21} weight="bold">Connections</Text><Text size={14} color="#6B7079">Approve systems, verify accounts and choose who can configure agent access.</Text>
<Frame w="fill" h="hug" flex="row" gap={28}>
<Frame w={490} h="hug" flex="col" gap={18}>
<Text size={16} weight="bold">Business systems</Text>
<Frame w="fill" h="hug" flex="col" gap={10} py={14}><Text size={16} weight="bold">Orders</Text><Text size={14} color="#6B7079">Account: Acme support</Text><Text size={14}>Approved · Connected · 3 reviewed operations</Text><Frame w="fill" h="hug" flex="row" gap={20}><Text size={14} weight="bold">Manage access</Text><Text size={14}>Disconnect</Text><Text size={14}>Remove approval</Text></Frame></Frame>
<Rectangle w="fill" h={1} fill="#E5E7EA" />
<Frame w="fill" h="hug" flex="col" gap={10} py={14}><Text size={16} weight="bold">Inventory</Text><Text size={14} color="#6B7079">Account: Acme warehouse</Text><Text size={14}>Not approved · Not connected</Text><Text size={14} weight="bold">Approve system</Text></Frame>
<Rectangle w="fill" h={1} fill="#E5E7EA" />
<Text size={16} weight="bold">Agent creators</Text><Text w="fill" size={14} color="#6B7079">Creation permission is separate from permission to use a connected account.</Text><Text size={14}>Priya · May create workspace agents</Text><Text size={14}>Ravi · Private agents only</Text>
</Frame>
<Frame w="fill" h="hug" flex="col" gap={16} p={20} bg="#F7F8F9" rounded={12}>
<Text size={18} weight="bold">Orders · Configure access</Text><Text size={14}>Person: Priya</Text><Text w="fill" size={14} color="#6B7079">Choose the maximum authority Priya may assign to her agents. New operations stay unavailable.</Text>
<Text size={14}>Read order</Text><Text size={14} weight="bold">May allow automatically</Text><Rectangle w="fill" h={1} fill="#E5E7EA" />
<Text size={14}>Update order</Text><Text size={14} weight="bold">Must ask me first</Text><Rectangle w="fill" h={1} fill="#E5E7EA" />
<Text size={14}>Delete order</Text><Text size={14} weight="bold">Unavailable</Text><Text w="fill" size={13} color="#6B7079">You remain the approver for actions that require permission.</Text>
<Frame w="fill" h={42} flex="row" items="center" justify="center" bg="#252A31" rounded={8}><Text size={14} color="#FFFFFF" weight="bold">Save access</Text></Frame>
</Frame></Frame></Frame></Frame>
